use seed::{prelude::*, *};

pub fn view<Ms>() -> Node<Ms> {
    section![
        C!["hero", "is-medium", "ml-6"],
        div![
            C!["hero-body"],
            h1![C!["title", "is-size-1"], "404",],
            h2![C!["subtitle", "is-size-3"], "Page not found",]
        ]
    ]
}
