// Run with: node --test test/
const test = require('node:test');
const assert = require('node:assert/strict');
require('../resources/core.js');
const C = globalThis.FocusCore;

const H = 60 * C.MIN;
const today = (h, m = 0) => { const d = new Date(); return new Date(d.getFullYear(), d.getMonth(), d.getDate(), h, m).getTime(); };
const daily = times => ({ ...C.defaults().caffeine, mode: 'daily', times });

test('first daily dose is due at its time', () => {
  assert.deepEqual(C.dailyNext(daily([540, 780]), today(8)), { dose: 0, due: today(9) });
});

test('late first dose pushes the second back by the same amount', () => {
  const c = daily([540, 780]);
  Object.assign(c, { lastTaken: today(11), takenToday: 1, takenDate: C.dateKey(today(11)) });
  assert.deepEqual(C.dailyNext(c, today(12)), { dose: 1, due: today(15) });
});

test('a dose missed by over 4 hours is skipped', () => {
  assert.deepEqual(C.dailyNext(daily([480, 840]), today(13)), { dose: 1, due: today(14) });
});

test('all doses done means tomorrow', () => {
  const c = daily([540]);
  Object.assign(c, { takenToday: 1, takenDate: C.dateKey(today(10)) });
  const r = C.dailyNext(c, today(10));
  assert.equal(r.dose, null);
  assert.equal(r.due, today(9) + 24 * H);
});

test("yesterday's count resets", () => {
  const c = daily([540]);
  Object.assign(c, { takenToday: 1, takenDate: '2000-1-1' });
  assert.equal(C.dailyNext(c, today(8)).dose, 0);
});

test('took it marks the right daily dose even when the first was missed', () => {
  const s = C.sanitize({ caffeine: { mode: 'daily', times: [480, 840] } }), rt = { caffeineSnooze: 1 };
  C.tookCaffeine(s, rt, today(14));
  assert.equal(s.caffeine.takenToday, 2);
  assert.equal(rt.caffeineSnooze, null);
});

test('interval caffeine only counts from when it was taken; not now snoozes 30 min', () => {
  const s = C.defaults(), rt = { startedAt: today(9), nextAt: { posture: 1e15, water: 1e15, walk: 1e15 }, caffeineSnooze: null };
  assert.equal(C.caffeineDue(s, rt, today(9)), today(9) + C.MIN);     // asks a minute after launch
  assert.deepEqual(C.due(s, rt, today(9, 1)), ['caffeine']);
  assert.equal(C.caffeineDue(s, rt, today(9, 1)), today(9, 31));     // re-asks in 30 min unless taken
  C.tookCaffeine(s, rt, today(9, 5));
  assert.equal(C.caffeineDue(s, rt, today(9, 5)), today(13, 5));     // 4 hr from taking it
});

test('due() queues each reminder once per interval and respects pause', () => {
  const s = C.defaults(), rt = { startedAt: 0, nextAt: { posture: 1000, water: 1e15, walk: 1e15 }, caffeineSnooze: 1e15 };
  assert.deepEqual(C.due(s, rt, 2000), ['posture']);
  assert.deepEqual(C.due(s, rt, 3000), []);
  s.paused = true; rt.nextAt.posture = 0;
  assert.deepEqual(C.due(s, rt, 4000), []);
});

test('sanitize repairs and migrates settings', () => {
  const s = C.sanitize({ posture: 30, water: 0, saved: { posture: [20, 30], walk: [5, 9999, 5] }, caffeine: { mode: 'x', times: [2000, 600, 600, 300, 900] } });
  assert.equal(s.every.posture, 30);
  assert.equal(s.every.water, 0);
  assert.deepEqual(s.saved.walk, [5, 60]);                // invalid dropped, active interval kept
  assert.equal(s.caffeine.mode, 'interval');
  assert.deepEqual(s.caffeine.times, [300, 600]);
  assert.equal(C.sanitize({ mins: 45 }).every.posture, 45); // posture-only version
  assert.deepEqual(C.sanitize(null), C.sanitize({}));
});

test('panel opens out to the left of the tray and stays on screen', () => {
  // 1920x1040 work area, taskbar at the bottom, click near the right edge, 800x500 panel.
  assert.deepEqual(C.panelOrigin({ x: 1800, y: 1000 }, [0, 0, 1920, 1040], [800, 500]), { x: 1016, y: 492 });
  // Click at the very right of a small screen: clamped inside.
  assert.deepEqual(C.panelOrigin({ x: 1360, y: 700 }, [0, 0, 1366, 728], [1000, 600]), { x: 358, y: 92 });
  // Top menu bar (macOS): below the click.
  assert.equal(C.panelOrigin({ x: 1700, y: 10 }, [0, 25, 1920, 1055], [800, 500]).y, 33);
  // No click known: bottom-right corner.
  assert.deepEqual(C.panelOrigin(null, [0, 0, 1920, 1040], [800, 500]), { x: 1112, y: 532 });
  // Panel bigger than the screen: pinned to the top-left of the work area rather than off-screen.
  assert.deepEqual(C.panelOrigin({ x: 500, y: 500 }, [0, 0, 600, 400], [800, 500]), { x: 8, y: 8 });
});

test('versions and minutes parse', () => {
  assert.ok(C.newer('4.0.10', '4.0.9'));
  assert.ok(!C.newer('4.0.9', '4.0.9'));
  assert.equal(C.parseMinutes('1.5h'), 90);
  assert.equal(C.parseMinutes('25'), 25);
  assert.equal(C.parseMinutes('abc'), null);
  assert.equal(C.parseMinutes('13h'), null);
  assert.equal(C.short(90), '1 hr 30 min');
});
