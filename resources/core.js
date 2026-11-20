// Focus scheduling and layout logic. No Neutralino calls here, so it can be tested with plain Node
// (see test/core.test.js). Times are unix ms; intervals are minutes; daily dose times are minutes
// after local midnight.
(function (root) {
  const KINDS = ['posture', 'water', 'walk', 'caffeine'];
  const MAX_SAVED = 5;
  const MAX_MINUTES = 720;
  const MIN = 60000;
  const CAFFEINE_SNOOZE = 30 * MIN;      // "Not now" or no answer: ask again this much later
  const CAFFEINE_MISS = 4 * 60 * MIN;    // a daily dose this late is skipped (late caffeine hurts sleep)

  function defaults() {
    return {
      every: { posture: 20, water: 45, walk: 60, caffeine: 240 },
      saved: { posture: [20, 30, 45], water: [30, 45, 60], walk: [60, 90], caffeine: [180, 240, 300] },
      paused: false,
      caffeine: { on: true, mode: 'interval', times: [9 * 60], lastTaken: null, takenDate: '', takenToday: 0 },
    };
  }

  const validMinutes = m => Number.isInteger(m) && m >= 1 && m <= MAX_MINUTES;

  function cleanTimes(t) {
    return [...new Set((Array.isArray(t) ? t : []).filter(m => Number.isInteger(m) && m >= 0 && m < 1440))]
      .sort((a, b) => a - b).slice(0, 2);
  }

  /** Repair anything odd in loaded settings instead of failing. Also migrates older versions. */
  function sanitize(raw) {
    const d = defaults();
    const s = raw && typeof raw === 'object' ? raw : {};
    if (typeof s.mins === 'number' && s.posture === undefined) s.posture = s.mins; // posture-only version
    const out = { every: {}, saved: {}, paused: !!s.paused, caffeine: { ...d.caffeine } };
    for (const k of KINDS) {
      // Older versions stored intervals at the top level ({ posture: 20, saved: {...} }).
      const every = s.every?.[k] ?? s[k];
      let saved = Array.isArray(s.saved?.[k]) ? s.saved[k] : d.saved[k];
      saved = [...new Set(saved.filter(validMinutes))].sort((a, b) => a - b).slice(0, MAX_SAVED);
      let e = every === 0 || validMinutes(every) ? every : d.every[k];
      if (e && !saved.includes(e)) saved = [...saved, e].sort((a, b) => a - b).slice(-MAX_SAVED);
      if (e && !saved.includes(e)) e = 0;
      out.every[k] = e;
      out.saved[k] = saved;
    }
    const c = s.caffeine && typeof s.caffeine === 'object' ? s.caffeine : {};
    out.caffeine.on = c.on !== undefined ? !!c.on : d.caffeine.on;
    out.caffeine.mode = c.mode === 'daily' ? 'daily' : 'interval';
    out.caffeine.times = cleanTimes(c.times);
    if (!out.caffeine.times.length) out.caffeine.times = [9 * 60];
    out.caffeine.lastTaken = Number.isFinite(c.lastTaken) ? c.lastTaken : null;
    out.caffeine.takenDate = typeof c.takenDate === 'string' ? c.takenDate : '';
    out.caffeine.takenToday = Number.isInteger(c.takenToday) ? c.takenToday : 0;
    return out;
  }

  // ---- local time helpers ----
  const dateKey = ms => { const d = new Date(ms); return `${d.getFullYear()}-${d.getMonth() + 1}-${d.getDate()}`; };
  function atMinutes(ms, minutes, dayOffset = 0) {
    const d = new Date(ms);
    return new Date(d.getFullYear(), d.getMonth(), d.getDate() + dayOffset, Math.floor(minutes / 60), minutes % 60).getTime();
  }

  /**
   * Daily caffeine: which dose is up (skipping ones missed by more than CAFFEINE_MISS) and when.
   * A late first dose pushes the second back by the same amount, keeping their spacing.
   */
  function dailyNext(c, now) {
    const done = c.takenDate === dateKey(now) ? c.takenToday : 0;
    const midnight = atMinutes(now, 0);
    const lastToday = c.lastTaken != null && c.lastTaken >= midnight ? c.lastTaken : null;
    for (let i = done; i < c.times.length; i++) {
      let due = atMinutes(now, c.times[i]);
      if (i > 0 && lastToday != null) due = Math.max(due, lastToday + (c.times[i] - c.times[i - 1]) * MIN);
      if (now <= due + CAFFEINE_MISS) return { dose: i, due };
    }
    return { dose: null, due: atMinutes(now, c.times[0], 1) };
  }

  /** When caffeine should next ask, or null when it's off. */
  function caffeineDue(s, rt, now) {
    const c = s.caffeine;
    if (!c.on) return null;
    let due;
    if (c.mode === 'daily') due = dailyNext(c, now).due;
    else {
      const every = s.every.caffeine;
      if (!every) return null;
      due = c.lastTaken != null ? c.lastTaken + every * MIN : rt.startedAt + MIN;
    }
    return Math.max(due, rt.caffeineSnooze || 0);
  }

  /** Record "Took it": the next countdown starts now. */
  function tookCaffeine(s, rt, now) {
    const c = s.caffeine;
    if (c.mode === 'daily') {
      const { dose } = dailyNext(c, now);
      c.takenToday = dose == null ? c.times.length : dose + 1;
      c.takenDate = dateKey(now);
    }
    c.lastTaken = now;
    rt.caffeineSnooze = null;
  }

  /** Restart a reminder's countdown. For caffeine in interval mode that means "count from now". */
  function reset(s, rt, kind, now) {
    if (kind === 'caffeine') {
      rt.caffeineSnooze = null;
      if (s.caffeine.mode === 'interval') s.caffeine.lastTaken = now;
      return;
    }
    rt.nextAt[kind] = now + (s.every[kind] || 0) * MIN;
  }

  /** Every few seconds: which reminders just came due (and get queued). Clock-based, so sleep never drifts it. */
  function due(s, rt, now) {
    if (s.paused) return [];
    const out = [];
    for (const k of KINDS) {
      if (k === 'caffeine') {
        const d = caffeineDue(s, rt, now);
        if (d != null && now >= d) { rt.caffeineSnooze = now + CAFFEINE_SNOOZE; out.push(k); }
        continue;
      }
      if (!s.every[k] || now < (rt.nextAt[k] || 0)) continue;
      rt.nextAt[k] = now + s.every[k] * MIN;
      out.push(k);
    }
    return out;
  }

  /** Next time for each reminder (for the panel's countdowns). */
  function nextTimes(s, rt, now) {
    const out = {};
    for (const k of KINDS) {
      if (k === 'caffeine') out[k] = caffeineDue(s, rt, now);
      else out[k] = s.every[k] ? rt.nextAt[k] : null;
    }
    return out;
  }

  /**
   * Where the panel goes, in native window units. Its right edge lines up with where the tray was
   * clicked so it opens out to the left; above that point when it's in the lower half of the screen
   * (taskbar at the bottom), below it otherwise (macOS menu bar); always fully inside `area`.
   */
  function panelOrigin(click, area, size, gap = 8) {
    const [ax, ay, aw, ah] = area, [pw, ph] = size;
    let x, y;
    if (click) {
      x = click.x + gap * 2 - pw;
      y = click.y > ay + ah / 2 ? click.y - ph - gap : click.y + gap;
    } else {
      x = ax + aw - pw - gap;
      y = ay + ah - ph - gap;
    }
    const clamp = (v, lo, hi) => (hi < lo ? lo : Math.min(Math.max(v, lo), hi));
    return { x: Math.round(clamp(x, ax + gap, ax + aw - pw - gap)), y: Math.round(clamp(y, ay + gap, ay + ah - ph - gap)) };
  }

  /** "1.2.10" > "1.2.9" */
  function newer(a, b) {
    const x = String(a).split('.').map(Number), y = String(b).split('.').map(Number);
    for (let i = 0; i < Math.max(x.length, y.length); i++) if ((x[i] || 0) !== (y[i] || 0)) return (x[i] || 0) > (y[i] || 0);
    return false;
  }

  /** "25", "25m", "1.5h", "2 hr" → minutes (1 min – 12 hr), or null. */
  function parseMinutes(v) {
    const m = /^\s*(\d+(?:\.\d+)?)\s*(m|min|mins|minutes?|h|hr|hrs|hours?)?\s*$/i.exec(v);
    if (!m) return null;
    const n = Math.round(parseFloat(m[1]) * (/^h/i.test(m[2] || '') ? 60 : 1));
    return validMinutes(n) ? n : null;
  }

  const short = m => (m === 0 ? 'Off' : m < 60 ? `${m} min` : m % 60 ? `${Math.floor(m / 60)} hr ${m % 60} min` : `${m / 60} hr`);

  root.FocusCore = {
    KINDS, MAX_SAVED, MIN, CAFFEINE_SNOOZE, CAFFEINE_MISS,
    defaults, sanitize, cleanTimes, dateKey, atMinutes, dailyNext, caffeineDue, tookCaffeine, reset, due, nextTimes,
    panelOrigin, newer, parseMinutes, short,
  };
})(typeof window !== 'undefined' ? window : globalThis);
