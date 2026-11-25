// First thing in <head>: the saved colour theme is applied before the first paint, the page learns
// that scripts run (the menu button and the filters only appear then), and which system the visitor
// is on (html[data-os]: the style sheet shows that system's download button, with no flicker).
(function () {
  var d = document.documentElement;
  d.classList.add("js");
  try {
    var t = localStorage.getItem("theme");
    if (t === "light" || t === "dark") d.dataset.theme = t;
  } catch (e) {}
  var ua = navigator.userAgent || "";
  var os = /Android/i.test(ua) ? "android"
    : /Windows/i.test(ua) ? (/ARM|aarch64/i.test(ua) ? "windows-arm" : "windows")
    : /iPhone|iPad|iPod/i.test(ua) ? ""
    : /Mac OS X|Macintosh/i.test(ua) ? "mac"
    : /Linux/i.test(ua) ? (/aarch64|arm64/i.test(ua) ? "linux-arm" : "linux") : "";
  if (os) d.dataset.os = os;
})();
