package org.openbus.game;

import android.annotation.SuppressLint;
import android.app.Activity;
import android.app.Dialog;
import android.view.KeyEvent;
import android.view.WindowManager;
import android.graphics.Color;
import android.net.Uri;
import android.view.View;
import android.view.ViewGroup;
import android.webkit.JavascriptInterface;
import android.webkit.WebResourceRequest;
import android.webkit.WebResourceResponse;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;

import java.io.ByteArrayInputStream;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.util.HashMap;
import java.util.Map;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/**
 * The launcher on a phone: the desktop launcher's page (launcher/ui, in the APK's assets
 * under launcher/) in a web view over the game's window. Its commands go to the game program
 * (android_launcher.rs, the same code the desktop launcher runs); the page's files, the
 * content's pictures and the bus preview's pictures are answered as requests to
 * http://omsi.localhost/. A duty it starts takes the window (the view steps aside) and gives
 * it back when the session ends.
 */
public final class LauncherBridge {
    private static final String HOST = "omsi.localhost";
    private static Activity activity;
    private static WebView view;
    /** The page's own window over the game's (NativeActivity gives its window's surface to
     * the game, so no view of that window is ever drawn). */
    private static Dialog sheet;
    /** The commands run here, a few at once (a content list must not hold up the preview). */
    private static final ExecutorService WORK = Executors.newFixedThreadPool(4);

    static {
        // (NativeActivity loads the game's library its own way: loaded here again, the
        // methods below find their native halves in it)
        System.loadLibrary("openbus_game");
    }

    static native String call(String cmd, String args);
    static native byte[] bytes(String cmd, String args);

    private LauncherBridge() {}

    /** Show the launcher page over the window (from the activity's onCreate). */
    @SuppressLint("SetJavaScriptEnabled")
    static void attach(Activity a) {
        activity = a;
        try {
            a.getAssets().open("launcher/ui/index.html").close();
        } catch (Exception e) {
            // (an APK without the page has no launcher: the game logs it)
            return;
        }
        WebView w = new WebView(a);
        w.setBackgroundColor(Color.rgb(18, 18, 18));
        WebSettings s = w.getSettings();
        s.setJavaScriptEnabled(true);
        s.setDomStorageEnabled(true);
        s.setAllowFileAccess(false);
        s.setAllowContentAccess(false);
        s.setMediaPlaybackRequiresUserGesture(true);
        w.addJavascriptInterface(new Page(), "OmsiLauncher");
        w.setWebViewClient(new WebViewClient() {
            @Override
            public WebResourceResponse shouldInterceptRequest(WebView v, WebResourceRequest r) {
                Uri u = r.getUrl();
                if (!HOST.equals(u.getHost())) {
                    return null;
                }
                return answer(u);
            }

            @Override
            public boolean shouldOverrideUrlLoading(WebView v, WebResourceRequest r) {
                // (a link to anything but the page goes to the browser)
                if (!HOST.equals(r.getUrl().getHost())) {
                    openUrl(r.getUrl().toString());
                    return true;
                }
                return false;
            }
        });
        Dialog d = new Dialog(a, android.R.style.Theme_Black_NoTitleBar_Fullscreen) {
            @Override
            public boolean dispatchKeyEvent(KeyEvent e) {
                // Back goes to the page (it closes its sheets) rather than closing the window
                if (e.getKeyCode() == KeyEvent.KEYCODE_BACK) {
                    if (e.getAction() == KeyEvent.ACTION_UP) {
                        js("window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))");
                    }
                    return true;
                }
                return super.dispatchKeyEvent(e);
            }
        };
        d.setContentView(w, new ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
        d.setCancelable(false);
        if (d.getWindow() != null) {
            d.getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
            d.getWindow().setLayout(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT);
            if (android.os.Build.VERSION.SDK_INT >= 28) {
                WindowManager.LayoutParams lp = d.getWindow().getAttributes();
                lp.layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
                d.getWindow().setAttributes(lp);
            }
            immersive(d);
        }
        w.loadUrl("http://" + HOST + "/ui/index.html");
        view = w;
        sheet = d;
        d.show();
    }

    /** Whether the page is up. */
    static boolean shown() {
        return sheet != null && sheet.isShowing();
    }

    @SuppressWarnings("deprecation")
    private static void immersive(Dialog d) {
        d.getWindow().getDecorView().setSystemUiVisibility(View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                | View.SYSTEM_UI_FLAG_FULLSCREEN
                | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                | View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
                | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN);
    }

    private static WebResourceResponse answer(Uri u) {
        String path = u.getPath() == null ? "" : u.getPath();
        Map<String, String> headers = new HashMap<>();
        headers.put("Access-Control-Allow-Origin", "*");
        try {
            if (path.startsWith("/ui/") || path.startsWith("/mock/")) {
                InputStream in = activity.getAssets().open("launcher" + path);
                return new WebResourceResponse(mime(path), mime(path).startsWith("text") || path.endsWith(".js") ? "utf-8" : null, 200, "OK", headers, in);
            }
            String cmd;
            String args;
            if (path.startsWith("/content/")) {
                cmd = "content_picture";
                args = "{\"path\":" + org.json.JSONObject.quote(path.substring("/content/".length())) + "}";
            } else if (path.startsWith("/bytes/")) {
                cmd = path.substring("/bytes/".length());
                args = u.getQueryParameter("args");
            } else {
                return status(404, "not found");
            }
            byte[] out = bytes(cmd, args == null ? "{}" : args);
            if (out == null || out.length == 0 || out[0] != 1) {
                String why = out == null || out.length < 2 ? "failed" : new String(out, 1, out.length - 1, StandardCharsets.UTF_8);
                return status(500, why);
            }
            return new WebResourceResponse(mime(path), null, 200, "OK", headers, new ByteArrayInputStream(out, 1, out.length - 1));
        } catch (Exception e) {
            return status(404, String.valueOf(e.getMessage()));
        }
    }

    private static WebResourceResponse status(int code, String text) {
        Map<String, String> headers = new HashMap<>();
        headers.put("Access-Control-Allow-Origin", "*");
        return new WebResourceResponse("text/plain", "utf-8", code, code == 404 ? "Not Found" : "Error", headers, new ByteArrayInputStream(text.getBytes(StandardCharsets.UTF_8)));
    }

    private static String mime(String path) {
        String p = path.toLowerCase();
        if (p.endsWith(".html")) return "text/html";
        if (p.endsWith(".js")) return "application/javascript";
        if (p.endsWith(".css")) return "text/css";
        if (p.endsWith(".json")) return "application/json";
        if (p.endsWith(".svg")) return "image/svg+xml";
        if (p.endsWith(".woff2")) return "font/woff2";
        if (p.endsWith(".png")) return "image/png";
        if (p.endsWith(".bmp")) return "image/bmp";
        if (p.endsWith(".jpg") || p.endsWith(".jpeg")) return "image/jpeg";
        return "application/octet-stream";
    }

    private static void js(final String code) {
        if (view == null) {
            return;
        }
        activity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                view.evaluateJavascript(code, null);
            }
        });
    }

    private static void openUrl(String url) {
        if (activity instanceof OmsiActivity) {
            ((OmsiActivity) activity).openUrl(url);
        }
    }

    // --- called from the game program (android_launcher.rs), any thread

    /** An event of the launcher (`vehicles-progress` ...) for the page. */
    static void emit(String event, String json) {
        js("window.dispatchEvent(new CustomEvent(" + org.json.JSONObject.quote("omsi:" + event) + ", { detail: " + json + " }))");
    }

    /** A duty was started: the window goes to the game. */
    static void launched() {
        if (view == null) {
            return;
        }
        activity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                sheet.hide();
            }
        });
    }

    /** The session ended: the page comes back, and hears that no game runs any more. */
    static void sessionEnded() {
        if (view == null) {
            return;
        }
        activity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                sheet.show();
                immersive(sheet);
                view.requestFocus();
                view.evaluateJavascript("window.dispatchEvent(new CustomEvent('omsi:games-ended', { detail: null }))", null);
            }
        });
    }

    /** What the page calls (`window.OmsiLauncher`). */
    static final class Page {
        @JavascriptInterface
        public void call(final int id, final String cmd, final String args) {
            WORK.execute(new Runnable() {
                @Override
                public void run() {
                    String r;
                    try {
                        r = page(cmd, args);
                    } catch (Throwable t) {
                        r = "0" + t;
                    }
                    boolean ok = r.startsWith("1");
                    js("window.__omsiReply(" + id + ", " + ok + ", " + org.json.JSONObject.quote(r.substring(1)) + ")");
                }
            });
        }
    }

    /** The phone's own answers to what the desktop window does itself; the rest is the game's. */
    private static String page(String cmd, String args) throws Exception {
        switch (cmd) {
            case "open_url": {
                openUrl(new org.json.JSONObject(args).getString("url"));
                return "1true";
            }
            case "pick_folder":
            case "pick_file":
            case "pick_mod":
                // (no file dialogs on a phone: mods go into openBus/Mods on the storage)
                return "1null";
            case "open_path":
            case "selftest_done":
                return "1null";
            default:
                return call(cmd, args);
        }
    }
}
