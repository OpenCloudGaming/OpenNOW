#pragma once

#include <SDL3/SDL_joystick.h>

inline bool setSmokeControllerButton(SDL_Joystick *joystick, int button, bool pressed)
{
    if (!joystick || !SDL_SetJoystickVirtualButton(joystick, button, pressed)) return false;
    SDL_UpdateJoysticks();
    return true;
}
