# This is a native-build configuration file, not a cross-compilation toolchain.
# sdl2-sys exposes CMake configuration through SDL2_TOOLCHAIN, so use it to
# keep the bundled library focused on the Pi's DRM/KMS console display path.
set(CMAKE_C_STANDARD 99 CACHE STRING "" FORCE)
set(SDL_X11 OFF CACHE BOOL "" FORCE)
set(SDL_WAYLAND OFF CACHE BOOL "" FORCE)
set(SDL_DIRECTFB OFF CACHE BOOL "" FORCE)
set(SDL_KMSDRM ON CACHE BOOL "" FORCE)
set(SDL_KMSDRM_SHARED ON CACHE BOOL "" FORCE)
set(SDL_OPENGL OFF CACHE BOOL "" FORCE)
set(SDL_OPENGLES ON CACHE BOOL "" FORCE)
set(SDL_VULKAN OFF CACHE BOOL "" FORCE)

# AstroHUD does not play audio. Avoid probing and linking unused sound stacks.
set(SDL_ALSA OFF CACHE BOOL "" FORCE)
set(SDL_JACK OFF CACHE BOOL "" FORCE)
set(SDL_PIPEWIRE OFF CACHE BOOL "" FORCE)
set(SDL_PULSEAUDIO OFF CACHE BOOL "" FORCE)
set(SDL_SNDIO OFF CACHE BOOL "" FORCE)
set(SDL_JOYSTICK OFF CACHE BOOL "" FORCE)
set(SDL_HAPTIC OFF CACHE BOOL "" FORCE)
set(SDL_HIDAPI OFF CACHE BOOL "" FORCE)
set(SDL_SENSOR OFF CACHE BOOL "" FORCE)
