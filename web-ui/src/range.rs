//! Range slider that reports the value the user settled on.

use dioxus::prelude::*;

/// `<input type="range">` that calls `on_commit` once per drag, when the user
/// lets go.
///
/// A plain `onchange` is not enough: WebKitGTK (the Linux desktop app's
/// webview) does not fire `change` after a touchscreen drag, only `input`
/// events followed by `pointerup`/`touchend`. So the last `input` value is
/// kept and committed on whichever of `change`, `pointerup` or `touchend`
/// comes first; the others find nothing pending.
#[component]
pub fn RangeSlider(
    class: String,
    min: i64,
    max: i64,
    value: i64,
    #[props(into)] aria_label: Option<String>,
    on_commit: EventHandler<String>,
) -> Element {
    let mut pending: Signal<Option<String>> = use_signal(|| None);
    let mut commit = move || {
        if let Some(v) = pending.take() {
            on_commit.call(v);
        }
    };
    rsx! {
        input {
            r#type: "range",
            class,
            min,
            max,
            value,
            aria_label,
            oninput: move |e: Event<FormData>| pending.set(Some(e.value())),
            onchange: move |_| commit(),
            onpointerup: move |_| commit(),
            ontouchend: move |_| commit(),
        }
    }
}
