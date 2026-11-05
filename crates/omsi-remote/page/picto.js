"use strict";

const PICTOS = {
  door: { s: "M12 6H36V42H12Z M24 6V42 M20 24H14 M17 21L14 24L17 27 M28 24H34 M31 21L34 24L31 27" },
  doors: { s: "M6 10H22V38H6Z M26 10H42V38H26Z M14 24H10 M12 22L10 24L12 26 M18 24H22 M34 24H30 M32 22L30 24L32 26 M38 24H42" },
  stopBrake: { s: "M24 7A17 17 0 1 1 23.9 7Z M18 15V33 M30 15V33 M18 24H30" },
  parking: { s: "M24 11A13 13 0 1 1 23.9 11Z M11 9A21 21 0 0 0 11 39 M37 9A21 21 0 0 1 37 39 M20 32V16H26A5 5 0 0 1 26 26H20" },
  hazard: { s: "M24 6L43 39H5Z M24 18L34 35H14Z" },
  lowBeam: { s: "M28 12C40 12 40 36 28 36Z M22 15L8 19 M22 22L8 26 M22 29L8 33" },
  highBeam: { s: "M28 12C40 12 40 36 28 36Z M22 15H8 M22 21H8 M22 27H8 M22 33H8" },
  rearFog: { s: "M20 12C8 12 8 36 20 36Z M26 16H42 M26 24H42 M26 32H42 M34 10C31 15 37 19 34 24C31 29 37 33 34 38" },
  saloon: { s: "M8 12H40 M24 12V17 M15 29C15 21 33 21 33 29Z M24 33V40 M14 33L10 38 M34 33L38 38" },
  driverLight: { s: "M17 8H31L37 22H11Z M24 26V36 M14 26L9 33 M34 26L39 33 M18 42H30" },
  wiper: { s: "M6 36C12 12 36 12 42 36 M24 38L13 17 M24 38L24 38" },
  kneel: { s: "M5 10H37A5 5 0 0 1 42 15V28H5Z M5 19H42 M13 31A3 3 0 1 1 12.9 31Z M33 31A3 3 0 1 1 32.9 31Z M24 35V44 M20 40L24 44L28 40" },
  ramp: { s: "M22 6A3 3 0 1 1 21.9 6Z M22 13V25H32L36 35 M22 18H30 M17 19A10 10 0 1 0 31 33" },
  pram: { s: "M8 20H36C36 30 30 33 22 33C14 33 8 29 8 20Z M8 20C8 12 16 9 22 9V20 M36 20L42 13 M14 39A3 3 0 1 1 13.9 39Z M30 39A3 3 0 1 1 29.9 39Z" },
  horn: { s: "M7 19V29H13L31 39V9L13 19Z M36 18C39 21 39 27 36 30 M40 13C46 19 46 29 40 35" },
  battery: { s: "M7 15H41V37H7Z M13 11H20V15 M28 11H35V15 M13 26H20 M16.5 22.5V29.5 M28 26H35" },
  engine: { s: "M11 17H31V22H38V34H31V37H17L11 31Z M17 12H27 M22 12V17 M5 22V30 M5 26H11" },
  engineStop: { s: "M11 17H31V22H38V34H31V37H17L11 31Z M17 12H27 M22 12V17 M6 8L42 42" },
  key: { s: "M14 18A8 8 0 1 1 13.9 18Z M21 22H42V28 M34 22V28" },
  ticket: { s: "M7 13H41V20A4 4 0 0 0 41 28V35H7V28A4 4 0 0 0 7 20Z M30 15V18 M30 22V26 M30 30V33" },
  microphone: { s: "M18 8H30V26A6 6 0 0 1 18 26Z M12 22C12 36 36 36 36 22 M24 34V41 M17 41H31" },
  bell: { s: "M13 33C16 28 16 23 16 20C16 14 20 10 24 10C28 10 32 14 32 20C32 23 32 28 35 33Z M24 7V10 M20 37C20 41 28 41 28 37" },
  turnLeft: { f: "M5 24L19 11V19H42V29H19V37Z" },
  turnRight: { f: "M43 24L29 11V19H6V29H29V37Z" },
  heat: { s: "M14 38C10 32 18 28 14 22C11 17 16 13 14 8 M24 38C20 32 28 28 24 22C21 17 26 13 24 8 M34 38C30 32 38 28 34 22C31 17 36 13 34 8" },
  fan: { s: "M24 21A3 3 0 1 1 23.9 21Z M24 21C22 14 26 7 31 9C35 11 30 18 26 22 M27 24C34 22 41 26 39 31C37 35 30 30 26 26 M21 26C17 32 9 33 9 28C9 23 17 24 22 25" },
  asr: { s: "M12 30A10 10 0 1 1 11.9 30Z M36 30A10 10 0 1 1 35.9 30Z M12 30H36 M8 8L40 44" },
  retarder: { s: "M24 6A18 18 0 1 1 23.9 6Z M24 12A12 12 0 1 1 23.9 12Z M24 12V36" },
  mirror: { s: "M10 10H38V30H10Z M24 30V40 M16 40H32 M15 22C18 19 22 24 25 21C28 18 31 22 33 20" },
};

Object.assign(PICTOS, {
  roofHatch: { s: "M6 30H42 M10 30L16 18H32L38 30 M16 18L22 10H34L32 18 M24 36V44 M20 40L24 44L28 40" },
  wrench: { s: "M30 8A9 9 0 0 0 22 21L8 35A3.5 3.5 0 0 0 13 40L27 26A9 9 0 0 0 40 18L34 22L28 20L26 14Z" },
  heater: { s: "M8 40H40 M12 40V22H36V40 M18 22V40 M24 22V40 M30 22V40 M16 16C14 12 18 10 16 6 M24 16C22 12 26 10 24 6 M32 16C30 12 34 10 32 6" },
  climate: { s: "M24 6V42 M8.4 15L39.6 33 M8.4 33L39.6 15 M20 8L24 12L28 8 M20 40L24 36L28 40 M9 20L14 18L12 13 M39 28L34 30L36 35 M9 28L14 30L12 35 M39 20L34 18L36 13" },
  gearBox: { s: "M10 8H38V40H10Z M18 32V16L30 32V16" },
  autoKneel: { s: "M5 12H37A5 5 0 0 1 42 17V28H5Z M13 31A3 3 0 1 1 12.9 31Z M33 31A3 3 0 1 1 32.9 31Z M14 38V45 M11 42L14 45L17 42 M34 38V45 M31 42L34 45L37 42 M19 8H29" },
  rampOut: { s: "M6 34H42 M26 34L42 24 M14 10A3 3 0 1 1 13.9 10Z M14 15V24H21L24 31 M14 19H20 M10 20A7 7 0 1 0 20 30" },
  mirrorHeat: { s: "M10 8H38V28H10Z M24 28V38 M17 38H31 M17 22C15 19 19 17 17 13 M24 22C22 19 26 17 24 13 M31 22C29 19 33 17 31 13" },
  fuel: { s: "M10 42V10A3 3 0 0 1 13 7H27A3 3 0 0 1 30 10V42 M7 42H33 M14 13H26V21H14Z M30 18H34A3 3 0 0 1 37 21V33A2 2 0 0 0 41 33V16L36 11" },
  oil: { s: "M6 22H18L24 18H36L42 14 M18 22V32H36L40 26 M36 18V26 M10 18V22 M42 30C40 33 40 35 42 36C44 35 44 33 42 30Z" },
  coolant: { s: "M24 6V30 M24 12H30 M24 18H30 M24 24H30 M24 30A6 6 0 1 0 24.1 30 M6 40C10 37 14 43 18 40C22 37 26 43 30 40C34 37 38 43 42 40" },
  pressure: { s: "M24 10A14 14 0 1 1 23.9 10Z M24 24L32 16 M14 40H34 M24 38V42" },
  uturn: { s: "M32 42V18A9 9 0 0 0 14 18V30 M8 24L14 31L20 24" },
  straight: { s: "M24 42V8 M15 17L24 8L33 17" },
  info: { s: "M24 6A18 18 0 1 1 23.9 6Z M24 21V34 M24 14V15" },
});

const GLYPHS = { D: "D", N: "N", R: "R", "1": "1", "2": "2", "3": "3" };

function picto(name) {
  const p = PICTOS[name];
  if (!p && !GLYPHS[name]) return null;
  const NS = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(NS, "svg");
  svg.setAttribute("viewBox", "0 0 48 48");
  svg.setAttribute("class", "picto");
  svg.setAttribute("aria-hidden", "true");
  if (!p) {
    const t = document.createElementNS(NS, "text");
    t.setAttribute("x", "24");
    t.setAttribute("y", "36");
    t.setAttribute("text-anchor", "middle");
    t.setAttribute("font-size", "34");
    t.setAttribute("font-weight", "700");
    t.setAttribute("fill", "currentColor");
    t.textContent = GLYPHS[name];
    svg.append(t);
    return svg;
  }
  const path = document.createElementNS(NS, "path");
  if (p.f) {
    path.setAttribute("d", p.f);
    path.setAttribute("fill", "currentColor");
  } else {
    path.setAttribute("d", p.s);
    path.setAttribute("fill", "none");
    path.setAttribute("stroke", "currentColor");
    path.setAttribute("stroke-width", "3");
    path.setAttribute("stroke-linecap", "round");
    path.setAttribute("stroke-linejoin", "round");
  }
  svg.append(path);
  return svg;
}
