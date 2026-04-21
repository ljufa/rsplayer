use crate::{navigate, CurrentPath};
use dioxus::prelude::*;

#[component]
pub fn NotFoundPage(route: String) -> Element {
    let CurrentPath(path) = use_context::<CurrentPath>();
    rsx! {
        div { class: "p-8 text-center",
            h1 { class: "text-4xl font-bold mb-4", "404" }
            p { class: "text-base-content/70", "Page not found: {route}" }
            a {
                href: "/",
                onclick: move |e| {
                    e.prevent_default();
                    navigate(path, "/");
                },
                button { class: "btn btn-primary mt-4", "Go home" }
            }
        }
    }
}
