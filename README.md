# 🌕 AstroHud [in-progress]

This repo is meant to be an extremely simple example for displaying images using websockets / actix / wasm

### Building 

1) Build the wasm from the astroview_wasm project
    ```
    wasm-pack build --target web --out-dir ../static/pkg
    ```

2) Build the rest service
    ```
    cargo run --package astro-hud-rest
    ```

3) Run the server
    ```
    make serve
    ```

4) Connect the client
    ```
    cargo run --bin astrohud-client futo-lookback.jpg 
    ```

### Todo:
- [ ] Make this easier to build
- [ ] See realtime preview of it working
- [ ] Try to generate a 3D map ...

