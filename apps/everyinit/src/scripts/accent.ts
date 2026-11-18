import { getDailyAccent, getLocalDayKey } from "../lib/accent";

const root = document.documentElement;
let activeDayKey = getLocalDayKey();
let midnightTimer: number | undefined;

function applyDailyAccent(now: Date = new Date()): void {
  const preset = getDailyAccent(now);
  root.dataset.accent = preset.id;
  root.style.setProperty("--color-lime", preset.accent);
  root.style.setProperty("--accent-strong", preset.strong);
}

function checkDayChange(): void {
  const now = new Date();
  const nextDayKey = getLocalDayKey(now);
  if (nextDayKey === activeDayKey) return;
  activeDayKey = nextDayKey;
  applyDailyAccent(now);
}

function scheduleMidnightCheck(): void {
  if (midnightTimer !== undefined) {
    window.clearTimeout(midnightTimer);
  }
  const now = new Date();
  const nextMidnight = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1, 0, 0, 0, 50);
  const delay = Math.max(250, nextMidnight.getTime() - now.getTime());
  midnightTimer = window.setTimeout(() => {
    checkDayChange();
    scheduleMidnightCheck();
  }, delay);
}

applyDailyAccent();
scheduleMidnightCheck();

// Enable smooth @property color transitions only after initial paint so load has zero flash.
requestAnimationFrame(() => {
  requestAnimationFrame(() => {
    root.classList.add("is-accent-ready");
  });
});

document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible") {
    checkDayChange();
    scheduleMidnightCheck();
  }
});

window.addEventListener("focus", () => {
  checkDayChange();
  scheduleMidnightCheck();
});
