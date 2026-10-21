/** Start-up splash: "sound made visible". Thousands of grains of sand settle onto the nodal lines of
 * a vibrating plate (Chladni figures) that keep morphing as the pitch rises; then the grains are
 * drawn into the Genos wordmark, hum there, and on dismiss detonate outward as an iris opens onto
 * the app. Runs from its own early entry so it paints before the main bundle has mounted. */

interface SplashApi {
  dismiss: () => void;
}
declare global {
  interface Window {
    __genosSplash?: SplashApi;
  }
}

const BG = "11,20,25";
const MIN_VISIBLE_MS = 2600;
const FAILSAFE_MS = 20000;
const GRAINS = 7500;
const FORM_AT = 350; // ms: grains start gathering into the wordmark
const FORM_LEN = 1000;
const WORD_SHARE = 0.62; // fraction of grains that become the wordmark

interface Grain {
  x: number; y: number; vx: number; vy: number;
  tx: number; ty: number; word: boolean; seed: number;
}

const ease = (p: number) => p * p * (3 - 2 * p);
const clamp01 = (n: number) => Math.min(1, Math.max(0, n));

/** The wordmark as geometric monoline strokes ("Genos": tall round G, low-case e n o s), drawn from
 * circles and arcs so the letterforms never depend on an installed font. Returns the filled pixels
 * so grains can be assigned to the strokes, plus layout for the subtitle. */
function wordPoints(W: number, H: number): { pts: Array<[number, number]>; baseline: number; u: number } {
  const u = Math.max(40, Math.min(W * 0.125, H * 0.2));
  const off = document.createElement("canvas");
  off.width = W; off.height = H;
  const c = off.getContext("2d");
  if (!c) return { pts: [], baseline: H / 2, u };
  const rad = (deg: number) => (deg * Math.PI) / 180;
  const gap = 0.24;
  const widths = [1.4, 1, 0.9, 1, 0.74];
  const total = widths.reduce((a, b) => a + b, 0) + gap * (widths.length - 1);
  const baseline = H / 2 + 0.7 * u;
  let x = W / 2 - (total * u) / 2;
  c.strokeStyle = "#fff";
  c.lineWidth = u * 0.06;
  c.lineCap = "round";
  c.lineJoin = "round";
  const at = (lx: number, ly: number): [number, number] => [x + lx * u, baseline + ly * u];
  const arc = (cx: number, cy: number, rx: number, ry: number, a0: number, a1: number, ccw: boolean) => {
    const [px, py] = at(cx, cy);
    c.ellipse(px, py, rx * u, ry * u, 0, rad(a0), rad(a1), ccw);
  };
  const line = (x0: number, y0: number, x1: number, y1: number) => {
    const [ax, ay] = at(x0, y0), [bx, by] = at(x1, y1);
    c.moveTo(ax, ay); c.lineTo(bx, by);
  };
  const glyphs: Array<() => void> = [
    () => { // G: open circle with an inward bar
      arc(0.7, -0.7, 0.7, 0.7, -38, 0, true);
      line(1.4, -0.7, 0.78, -0.7);
    },
    () => { // e: circle with a bar, opening at lower right
      arc(0.5, -0.5, 0.5, 0.5, 0, 38, true);
      line(0, -0.5, 1, -0.5);
    },
    () => { // n: stem and arch
      line(0, 0, 0, -1);
      const [sx, sy] = at(0, -0.55);
      c.moveTo(sx, sy);
      arc(0.45, -0.55, 0.45, 0.45, 180, 360, false);
      line(0.9, -0.55, 0.9, 0);
    },
    () => arc(0.5, -0.5, 0.5, 0.5, 0, 360, false), // o
    () => { // s: two stacked half-ellipses
      arc(0.37, -0.75, 0.37, 0.25, -25, 90, true);
      arc(0.37, -0.25, 0.37, 0.25, -90, 155, false);
    },
  ];
  glyphs.forEach((draw, i) => {
    c.beginPath();
    draw();
    c.stroke();
    x += (widths[i] + gap) * u;
  });
  const data = c.getImageData(0, 0, W, H).data;
  const step = 2;
  const pts: Array<[number, number]> = [];
  for (let y = 0; y < H; y += step)
    for (let xx = 0; xx < W; xx += step)
      if (data[(y * W + xx) * 4 + 3] > 128) pts.push([xx, y]);
  return { pts, baseline, u };
}

function start(root: HTMLElement) {
  const canvas = root.querySelector("canvas") as HTMLCanvasElement;
  const sub = root.querySelector(".sub") as HTMLElement | null;
  const ctx = canvas.getContext("2d", { alpha: false });
  if (!ctx) return;
  const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;

  let W = 0, H = 0;
  let grains: Grain[] = [];
  let points: Array<[number, number]> = [];

  const build = () => {
    const dpr = Math.min(1.5, devicePixelRatio || 1);
    W = innerWidth; H = innerHeight;
    canvas.width = Math.round(W * dpr);
    canvas.height = Math.round(H * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.fillStyle = `rgb(${BG})`;
    ctx.fillRect(0, 0, W, H);
    const word = wordPoints(W, H);
    points = word.pts;
    if (sub) sub.style.top = `${word.baseline + word.u * 0.55}px`;
    const wordCount = Math.min(points.length, Math.floor(GRAINS * WORD_SHARE));
    grains = Array.from({ length: GRAINS }, (_, i) => {
      const word = i < wordCount && points.length > 0;
      const [tx, ty] = word ? points[(i * 7919) % points.length] : [0, 0];
      return {
        x: Math.random() * W, y: Math.random() * H, vx: 0, vy: 0,
        tx, ty, word, seed: Math.random() * 6.283,
      };
    });
  };
  build();
  addEventListener("resize", build);

  const t0 = performance.now();
  let raf = 0, running = true, bursting = false, burstAt = 0;
  let subShown = false;

  const frame = (now: number) => {
    if (!running) return;
    const t = reduced ? FORM_AT + FORM_LEN + 400 : now - t0;
    // Translucent fill leaves short glowing trails behind moving grains.
    ctx.fillStyle = `rgba(${BG},${bursting ? 0.22 : 0.3})`;
    ctx.fillRect(0, 0, W, H);

    // Plate modes rise over time, so the figure keeps reorganising like a sweep of rising pitch.
    const rise = Math.min(t, 2600) / 1000;
    const n = 2 + rise * 1.35;
    const m = n + 1.6 + Math.sin(t * 0.0009) * 0.8;
    const form = bursting ? 0 : ease(clamp01((t - FORM_AT) / FORM_LEN));
    
    const sweep = ((t * 0.00045) % 1.6 - 0.3) * W;
    const pi = Math.PI;
    const burstAge = bursting ? (now - burstAt) / 1000 : 0;

    if (!subShown && t > FORM_AT + FORM_LEN * 0.7 && sub) {
      subShown = true;
      sub.classList.add("on");
    }

    for (const g of grains) {
      let ax = 0, ay = 0, bright = 0.35, hue = 0;
      if (bursting && g.word) {
        // Wordmark grains are flung outward from the centre, faster the closer they were to it.
        const dx = g.x - W / 2, dy = g.y - H / 2;
        const d = Math.hypot(dx, dy) + 1;
        ax = (dx / d) * (0.9 + g.seed * 0.25);
        ay = (dy / d) * (0.9 + g.seed * 0.25);
        g.vx *= 1.015; g.vy *= 1.015;
        bright = 1; hue = 0.6;
      } else {
        // Chladni force: grains slide toward zeros of the plate's displacement field.
        const u = g.x / W, v = g.y / H;
        const a = Math.cos(n * pi * u), b = Math.cos(m * pi * v);
        const c = Math.cos(m * pi * u), d = Math.cos(n * pi * v);
        const f = a * b - c * d;
        const fx = -n * pi * Math.sin(n * pi * u) * b + m * pi * Math.sin(m * pi * u) * d;
        const fy = -m * pi * a * Math.sin(m * pi * v) + n * pi * c * Math.sin(n * pi * v);
        const w = g.word ? 1 - form : 1;
        ax += -f * fx * 0.9 * w;
        ay += -f * fy * 0.9 * w;
        const shake = Math.abs(f) * 2.2 * w;
        ax += (Math.random() - 0.5) * shake;
        ay += (Math.random() - 0.5) * shake;
        bright = 0.3 + Math.min(0.6, Math.abs(f) * 0.5) * w;
        if (g.word && form > 0) {
          // Living sand: a constant hum, plus gusts that lift grains off the strokes as the shimmer
          // passes and let them rain back into place.
          const gust = Math.exp(-(((g.tx - sweep) / (W * 0.05)) ** 2));
          const hum = 0.55 + gust * 5;
          const hx = g.tx + Math.sin(t * 0.012 + g.seed) * hum;
          const hy = g.ty + Math.cos(t * 0.011 + g.seed * 1.7) * hum - gust * (4 + 14 * Math.abs(Math.sin(g.seed * 5)));
          ax += (hx - g.x) * 0.012 * form;
          ay += (hy - g.y) * 0.012 * form;
          g.vx *= 0.86 + 0.1 * (1 - form);
          g.vy *= 0.86 + 0.1 * (1 - form);
          const lit = Math.max(0, 1 - Math.abs(g.tx - sweep) / (W * 0.07));
          bright = 0.55 + form * 0.3 + lit * 0.6;
          hue = clamp01(0.25 + lit);
        } else {
          hue = Math.min(1, Math.hypot(g.vx, g.vy) * 0.5);
        }
      }
      g.vx = (g.vx + ax) * 0.9;
      g.vy = (g.vy + ay) * 0.9;
      if (bursting && g.word) { g.vx += ax * 0.6; g.vy += ay * 0.6; }
      g.x += g.vx * (bursting && g.word ? 2.2 : 1) + (g.word ? 0 : 0);
      g.y += g.vy * (bursting && g.word ? 2.2 : 1);
      if (!g.word || !bursting) {
        if (g.x < 0) g.x += W; else if (g.x > W) g.x -= W;
        if (g.y < 0) g.y += H; else if (g.y > H) g.y -= H;
      }
      // Teal when calm, amber when excited or lit by the shimmer.
      const r = Math.round(47 + (237 - 47) * hue);
      const gC = Math.round(208 + (204 - 208) * hue);
      const bC = Math.round(192 + (150 - 192) * hue);
      const alpha = bursting ? Math.max(0, 1 - burstAge * 1.15) * bright : bright;
      ctx.fillStyle = `rgba(${r},${gC},${bC},${alpha})`;
      const sz = g.word && form > 0.5 ? 1.6 : 1.3;
      ctx.fillRect(g.x, g.y, sz, sz);
    }

    if (!reduced || !bursting) raf = requestAnimationFrame(frame);
  };
  raf = requestAnimationFrame(frame);

  let closing = false;
  const finish = () => {
    running = false;
    cancelAnimationFrame(raf);
    removeEventListener("resize", build);
    root.remove();
  };
  const dismiss = () => {
    if (closing) return;
    closing = true;
    const wait = Math.max(0, (reduced ? 500 : MIN_VISIBLE_MS) - (performance.now() - t0));
    setTimeout(() => {
      // Detonate the wordmark and open an iris onto the UI beneath.
      bursting = true;
      burstAt = performance.now();
      if (sub) sub.style.opacity = "0";
      const reach = Math.hypot(innerWidth, innerHeight) / 2 + 200;
      const dur = 1100;
      const open = (now: number) => {
        const p = clamp01((now - burstAt - 120) / dur);
        const e = p < 0.5 ? 2 * p * p : 1 - Math.pow(-2 * p + 2, 2) / 2;
        root.style.setProperty("--r", `${-200 + e * (reach + 200)}px`);
        if (p < 1) requestAnimationFrame(open);
        else finish();
      };
      requestAnimationFrame(open);
    }, wait);
  };
  window.__genosSplash = { dismiss };
  setTimeout(dismiss, FAILSAFE_MS);
}

const el = document.getElementById("splash");
const detached = new URLSearchParams(location.search).has("detach");
if (el && detached) el.remove();
else if (el) start(el);
