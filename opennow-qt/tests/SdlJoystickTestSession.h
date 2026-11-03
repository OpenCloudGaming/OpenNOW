#pragma once

#include <SDL3/SDL.h>

class SdlJoystickTestSession final
{
public:
    SdlJoystickTestSession()
    {
        SDL_SetHint(SDL_HINT_JOYSTICK_ALLOW_BACKGROUND_EVENTS, "1");
        SDL_SetHint(SDL_HINT_HIDAPI_ENUMERATE_ONLY_CONTROLLERS, "0");
        m_ready = SDL_InitSubSystem(SDL_INIT_JOYSTICK);
    }

    ~SdlJoystickTestSession()
    {
        if (m_ready) SDL_QuitSubSystem(SDL_INIT_JOYSTICK);
    }

    SdlJoystickTestSession(const SdlJoystickTestSession &) = delete;
    SdlJoystickTestSession &operator=(const SdlJoystickTestSession &) = delete;

    bool ready() const { return m_ready; }

    bool reset()
    {
        int count = 0;
        auto *ids = SDL_GetJoysticks(&count);
        if (!ids) return false;
        for (int index = 0; index < count; ++index) {
            if (SDL_IsJoystickVirtual(ids[index]) && !SDL_DetachVirtualJoystick(ids[index])) {
                SDL_free(ids);
                return false;
            }
        }
        SDL_free(ids);
        return SDL_ReloadGamepadMappings();
    }

private:
    bool m_ready = false;
};
