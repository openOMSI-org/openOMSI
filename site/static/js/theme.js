// First thing in <head>: the saved colour theme is applied before the first paint, and the page
// learns that scripts run (the menu button and the filters only appear then).
(function () {
  var d = document.documentElement;
  d.classList.add("js");
  try {
    var t = localStorage.getItem("theme");
    if (t === "light" || t === "dark") d.dataset.theme = t;
  } catch (e) {}
})();
