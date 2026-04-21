use crate::{navigate, CurrentPath};
use dioxus::prelude::*;

#[component]
pub fn HomePage() -> Element {
    let CurrentPath(path) = use_context::<CurrentPath>();
    rsx! {
        section { class: "hero min-h-dvh bg-base-200",
            div { class: "hero-content text-center",
                div { class: "max-w-md",
                    h1 { class: "text-5xl font-bold", "Welcome to RSPlayer" }
                    p { class: "py-6", "Please go to Settings to complete configuration." }
                    a {
                        href: "/settings",
                        onclick: move |e| {
                            e.prevent_default();
                            navigate(path, "/settings");
                        },
                        button { class: "btn btn-primary", "Go to Settings" }
                    }
                }
            }
        }
    }
}
