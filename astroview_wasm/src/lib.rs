use wasm_bindgen::prelude::*;
use web_sys::{ Event, HtmlImageElement, WebSocket};
use js_sys::Reflect;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

#[wasm_bindgen]
pub fn start_viewer() {
    console_error_panic_hook::set_once();

    let window = web_sys::window().expect("no global window exists");
    let document = window.document().expect("should have a document on window");
    let body = document.body().expect("document should have a body");

    // Create the img element as an Element first
    let img_element = document
        .create_element("img")
        .expect("failed to create img element");
    // img_element
    //     .style()
    //     .set_property("max-width", "100%")
    //     .expect("failed to set max-width style");

        let img = img_element
        .dyn_into::<HtmlImageElement>()
        .expect("failed to cast to HtmlImageElement");
    body.append_child(&img).expect("failed to append image");

    let heading = document
        .create_element("h1")
        .expect("failed to create h1 element")
        .dyn_into::<web_sys::HtmlElement>()
        .expect("failed to cast to HtmlElement");
    heading.set_inner_html("Latest Uploaded Image");
    body.insert_before(&heading, Some(&img)).expect("failed to insert heading");

    let ws = WebSocket::new("ws://192.168.88.118:8080/ws/").expect("failed to create WebSocket");

    Reflect::set(
        &ws,
        &JsValue::from_str("binaryType"),
        &JsValue::from_str("blob"),
    ).expect("failed to set binaryType");

    let img_clone = img.clone();
    let onmessage_callback = Closure::wrap(Box::new(move |e: web_sys::MessageEvent| {
        if let Ok(blob) = e.data().dyn_into::<web_sys::Blob>() {
            let url = web_sys::Url::create_object_url_with_blob(&blob).unwrap();
            img_clone.set_src(&url);
            log("New image received and displayed");
        }
    }) as Box<dyn FnMut(_)>);

    ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
    onmessage_callback.forget();

    let onerror_callback = Closure::wrap(Box::new(|e: Event| {
        log(&format!("WebSocket error: {:?}", e));
    }) as Box<dyn FnMut(_)>);
    ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
    onerror_callback.forget();

    let onopen_callback = Closure::wrap(Box::new(|_: Event| {
        log("WebSocket connection opened");
    }) as Box<dyn FnMut(_)>);
    ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
    onopen_callback.forget();
}