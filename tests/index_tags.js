// The window's stylesheets and scripts, in ui/index.html's order, for a check page opened from file:// (Chrome needs
// --allow-file-access-from-files to read index.html). Written in where this script stands, so the page's own
// scripts after it run once the window's have: index.html keeps the list, once.
(() => {
  const ui = new URL("../ui/", document.currentScript.src);
  const req = new XMLHttpRequest();
  req.open("GET", new URL("index.html", ui), false);
  req.send();
  const index = new DOMParser().parseFromString(req.responseText, "text/html");
  const tag = (el) => el.tagName === "LINK"
    ? `<link rel="stylesheet" href="${new URL(el.getAttribute("href"), ui)}">`
    : `<script src="${new URL(el.getAttribute("src"), ui)}"></` + "script>";
  document.write([...index.querySelectorAll('link[rel="stylesheet"], script[src]')].map(tag).join("\n"));
})();
