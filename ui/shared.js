// Shared by the panel and the reminder popup.
// Each reminder: its name, popup title, rotating lines and stick figure (same line style throughout).

const TYPES = {
  posture: {
    name: 'Posture', title: 'Head up.',
    lines: [
      'Bad posture causes a forward-leaning neck.',
      'Forward head posture strains your neck.',
      'Slouching pulls your head forward. Sit tall.',
      'Ears over shoulders. Your neck will thank you.',
      'Tech neck creeps in slowly. Reset now.',
      'Chin back. Let your head stack over your spine.',
    ],
  },
  water: {
    name: 'Water', title: 'Drink some water.',
    lines: [
      'Even mild dehydration can dull your focus.',
      'Sip before you feel thirsty.',
      'A glass now beats a headache later.',
      'Keep a bottle on your desk. Make it easy.',
      'Your brain runs on water. Top it up.',
    ],
  },
  walk: {
    name: 'Walk', title: 'Time for a walk.',
    lines: [
      'A few minutes on your feet resets your focus.',
      'Hours of sitting stiffen your back and hips.',
      'Stretch your legs. Rest your eyes.',
      'Walk to refill your water. Two birds, one stone.',
      'Look at something far away while you move.',
    ],
  },
};

const FIGURES = {
  "posture": "<svg viewBox=\"0 0 72 76\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"> <g class=\"ghost\"> <path d=\"M24 30 Q28 25 33 23\"/> <circle cx=\"38\" cy=\"19\" r=\"6.5\"/> </g> <path class=\"cue\" d=\"M37 8 Q32 4 28 6.5 M28 6.5 l3.2 -.6 M28 6.5 l1 3\"/> <line class=\"plumb\" x1=\"24\" y1=\"24\" x2=\"24\" y2=\"52\"/> <circle cx=\"24\" cy=\"15\" r=\"6.5\"/> <path d=\"M24 21.5 V28 Q21.5 40 23 52\"/> <path d=\"M24 32 L29 44 H44\"/> <path d=\"M23 52 H40 V70\"/> <path d=\"M40 44 H68\" class=\"desk\"/> <rect x=\"55\" y=\"24\" width=\"12\" height=\"14\" rx=\".5\" class=\"desk\"/> <path d=\"M61 38 V44\" class=\"desk\"/> </svg>",
  "water": "<svg viewBox=\"0 0 72 76\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"> <circle cx=\"24\" cy=\"14\" r=\"6.5\"/> <path d=\"M24 20.5 V46\"/> <path d=\"M24 46 L18 70 M24 46 L30 70\"/> <path d=\"M24 28 L19 45\"/> <path d=\"M24 28 L38 31 L37 22\"/> <path d=\"M33 11 L41 12 L39.5 22 L34 21.5 Z\"/> <path d=\"M31.5 13.5 L33.4 13.2\" stroke-width=\"1.5\"/> <g class=\"ghost\"> <path d=\"M49 12 q2.6 3.6 0 6 q-2.6 -2.4 0 -6 Z\"/> <path d=\"M55 23 q2 2.8 0 4.6 q-2 -1.8 0 -4.6 Z\"/> </g> <path class=\"desk\" d=\"M8 70 H48\"/> </svg>",
  "walk": "<svg viewBox=\"0 0 72 76\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"> <circle cx=\"34\" cy=\"12\" r=\"6.5\"/> <path d=\"M33 18.5 L30 44\"/> <path d=\"M30 44 L23 57 L17 69\"/> <path d=\"M30 44 L39 56 L45 68\"/> <path d=\"M32 26 L25 35 L21 42\"/> <path d=\"M32 26 L40 33 L46 38\"/> <g class=\"ghost\"> <path d=\"M5 24 H13 M3 32 H11 M7 40 H13\"/> </g> <path class=\"desk\" d=\"M8 70 H62\"/> </svg>"
};

function figure(kind) {
  const t = document.createElement('template');
  t.innerHTML = FIGURES[kind];
  return t.content;
}
