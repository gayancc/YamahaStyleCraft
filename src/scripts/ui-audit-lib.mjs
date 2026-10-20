// Shared live-window checks for the FL compliance audit (CDP 9449 debug launch).
export async function contrastReport(page, min = 4.5) {
  return page.evaluate((min) => {
    const parse = (c) => {
      const m = c.match(/rgba?\(([^)]+)\)/);
      if (!m) return null;
      const [r, g, b, a = 1] = m[1].split(/[ ,/]+/).filter(Boolean).map(Number);
      return { r, g, b, a };
    };
    const blend = (top, bottom) => ({
      r: top.r * top.a + bottom.r * (1 - top.a),
      g: top.g * top.a + bottom.g * (1 - top.a),
      b: top.b * top.a + bottom.b * (1 - top.a),
      a: 1,
    });
    const lum = ({ r, g, b }) => {
      const f = (v) => {
        v /= 255;
        return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
      };
      return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
    };
    const ratio = (a, b) => {
      const [hi, lo] = [lum(a), lum(b)].sort((x, y) => y - x);
      return (hi + 0.05) / (lo + 0.05);
    };
    // Candidate backgrounds behind an element. A gradient contributes each of its colour stops, so the
    // worst case across the gradient is what gets checked (background-color alone misses gradients).
    const backgroundsOf = (el) => {
      const chain = [];
      for (let n = el; n; n = n.parentElement) chain.push(n);
      let bases = [{ r: 21, g: 23, b: 25, a: 1 }];
      for (const n of chain.reverse()) {
        const cs = getComputedStyle(n);
        const c = parse(cs.backgroundColor);
        if (c && c.a > 0) bases = bases.map((b) => blend(c, b));
        const stops = [...(cs.backgroundImage || "").matchAll(/rgba?\([^)]+\)/g)]
          .map((m) => parse(m[0]))
          .filter((s) => s && s.a > 0);
        if (stops.length) bases = stops.flatMap((s) => bases.map((b) => blend(s, b)));
      }
      return bases;
    };
    const failures = [];
    let checked = 0;
    for (const el of document.querySelectorAll("body *")) {
      const own = [...el.childNodes]
        .filter((n) => n.nodeType === 3 && n.textContent.trim())
        .map((n) => n.textContent.trim())
        .join(" ");
      if (!own) continue;
      // Purely decorative punctuation separators carry no information.
      if (/^[\s·•/|\-–—:]+$/.test(own)) continue;
      const cs = getComputedStyle(el);
      const rect = el.getBoundingClientRect();
      if (
        cs.visibility === "hidden" ||
        cs.display === "none" ||
        rect.width < 1 ||
        rect.height < 1 ||
        Number(cs.opacity) === 0
      )
        continue;
      const fg = parse(cs.color);
      if (!fg) continue;
      const bgs = backgroundsOf(el);
      const r = Math.min(...bgs.map((bg) => ratio(fg.a < 1 ? blend(fg, bg) : fg, bg)));
      checked++;
      const size = parseFloat(cs.fontSize);
      const large = size >= 18.66 || (size >= 14 && Number(cs.fontWeight) >= 700);
      const need = large ? Math.min(min, 3) : min;
      if (r < need) {
        failures.push({
          ratio: Number(r.toFixed(2)),
          need,
          text: own.slice(0, 40),
          selector:
            el.tagName.toLowerCase() +
            (el.className && typeof el.className === "string"
              ? "." + el.className.trim().split(/\s+/).slice(0, 2).join(".")
              : ""),
          disabled: !!el.closest(":disabled, [aria-disabled='true']"),
          fg: cs.color,
          size,
        });
      }
    }
    return { checked, failures };
  }, min);
}

/** Elements whose box leaves the viewport or whose text is clipped horizontally. */
export async function layoutReport(page) {
  return page.evaluate(() => {
    const vw = innerWidth;
    const vh = innerHeight;
    const issues = [];
    const label = (el) =>
      el.tagName.toLowerCase() +
      (typeof el.className === "string" && el.className.trim()
        ? "." + el.className.trim().split(/\s+/).slice(0, 2).join(".")
        : "");
    const panels = document.querySelectorAll(
      "[role=dialog], .modal, .studio-menu-content, .floating-tool, .floating-tool-window",
    );
    for (const el of panels) {
      const cs = getComputedStyle(el);
      if (cs.display === "none" || cs.visibility === "hidden") continue;
      const r = el.getBoundingClientRect();
      if (r.width < 1) continue;
      if (r.left < -1 || r.top < -1 || r.right > vw + 1 || r.bottom > vh + 1)
        issues.push({ kind: "outside-viewport", el: label(el), rect: [r.left, r.top, r.right, r.bottom].map(Math.round) });
    }
    for (const el of document.querySelectorAll("button, label, h1, h2, h3, summary, select, input, .hint-name")) {
      const cs = getComputedStyle(el);
      if (cs.display === "none" || cs.visibility === "hidden") continue;
      const r = el.getBoundingClientRect();
      if (r.width < 1 || r.height < 1) continue;
      if (cs.overflow === "hidden" && cs.textOverflow !== "ellipsis" && el.scrollWidth > el.clientWidth + 1 && (el.textContent || "").trim())
        issues.push({ kind: "clipped-text", el: label(el), text: (el.textContent || "").trim().slice(0, 40), scroll: el.scrollWidth, client: el.clientWidth });
    }
    if (document.documentElement.scrollWidth > vw + 1)
      issues.push({ kind: "page-horizontal-scroll", scrollWidth: document.documentElement.scrollWidth, viewport: vw });
    return issues;
  });
}
