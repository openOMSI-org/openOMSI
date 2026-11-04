const heads = [...document.querySelectorAll(".doc h2")];
const toc = document.querySelector(".toc");
const title = document.createElement("p");
title.textContent = "On this page";
toc.append(title);
const links = heads.map(h => {
  h.id = h.textContent.toLowerCase().replace(/^\d+\.\s*/, "").replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
  const a = document.createElement("a");
  a.href = `#${h.id}`;
  a.textContent = h.textContent;
  toc.append(a);
  return a;
});
let queued = false;
function spy() {
  queued = false;
  const atEnd = innerHeight + scrollY >= document.documentElement.scrollHeight - 2;
  const current = atEnd ? heads.length - 1 : Math.max(0, heads.filter(h => h.getBoundingClientRect().top < 120).length - 1);
  links.forEach((a, i) => a.classList.toggle("active", i === current));
}
addEventListener("scroll", () => { if (!queued) { queued = true; requestAnimationFrame(spy); } }, { passive: true });
spy();
