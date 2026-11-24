// The docs menu keeps its scroll position from page to page (it is a new page each time), and
// scrolls to the current entry only when that is out of sight. Runs right after the menu, before paint.
(function () {
  var nav = document.getElementById("docs-nav");
  if (!nav) return;
  var key = "docs-nav-scroll";
  try { nav.scrollTop = Number(sessionStorage.getItem(key)) || 0; } catch (e) {}
  var cur = nav.querySelector("[aria-current=page]");
  if (cur) {
    var n = nav.getBoundingClientRect(), c = cur.getBoundingClientRect();
    if (c.top < n.top + 40 || c.bottom > n.bottom - 40) nav.scrollTop += c.top - n.top - n.height / 3;
  }
  var save = function () { try { sessionStorage.setItem(key, String(nav.scrollTop)); } catch (e) {} };
  nav.addEventListener("scroll", save, { passive: true });
  addEventListener("pagehide", save);
})();
