export const controllerButton = {
  south: 1 << 0,
  east: 1 << 1,
  west: 1 << 2,
  north: 1 << 3,
  leftShoulder: 1 << 4,
  rightShoulder: 1 << 5,
  up: 1 << 6,
  down: 1 << 7,
  left: 1 << 8,
  right: 1 << 9,
  menu: 1 << 10,
} as const;

export function readControllerGamepadButtons(pad: Gamepad | undefined): number {
  if (!pad) return 0;
  let buttons = 0;
  if (pad.buttons[0]?.pressed) buttons |= controllerButton.south;
  if (pad.buttons[1]?.pressed) buttons |= controllerButton.east;
  if (pad.buttons[2]?.pressed) buttons |= controllerButton.west;
  if (pad.buttons[3]?.pressed) buttons |= controllerButton.north;
  if (pad.buttons[4]?.pressed) buttons |= controllerButton.leftShoulder;
  if (pad.buttons[5]?.pressed) buttons |= controllerButton.rightShoulder;
  if (pad.buttons[12]?.pressed || (pad.axes[1] ?? 0) < -0.65) buttons |= controllerButton.up;
  if (pad.buttons[13]?.pressed || (pad.axes[1] ?? 0) > 0.65) buttons |= controllerButton.down;
  if (pad.buttons[14]?.pressed || (pad.axes[0] ?? 0) < -0.65) buttons |= controllerButton.left;
  if (pad.buttons[15]?.pressed || (pad.axes[0] ?? 0) > 0.65) buttons |= controllerButton.right;
  if (pad.buttons[9]?.pressed || pad.buttons[16]?.pressed) buttons |= controllerButton.menu;
  return buttons;
}
