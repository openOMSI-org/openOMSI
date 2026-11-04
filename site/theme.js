try { const t = localStorage.getItem("theme"); if (t === "light" || t === "dark") document.documentElement.dataset.theme = t; } catch {}
for (const href of ["https://fonts.googleapis.com/css2?family=Inter:wght@400..800&family=JetBrains+Mono:wght@500;700&display=swap", "https://fonts.googleapis.com/css2?family=Material+Icons+Round&display=block"]) {
  const link = document.createElement("link");
  link.rel = "stylesheet";
  link.href = href;
  document.head.appendChild(link);
}
