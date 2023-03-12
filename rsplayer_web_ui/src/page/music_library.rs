use seed::{
    a, article, attrs, button, div, empty, figure, i, id, img, input, li, log, nav, p, prelude::*,
    progress, raw, span, struct_urls, style, ul, C, IF,
};

#[derive(Debug)]
pub enum Msg {}

#[derive(Debug)]
pub struct Model {}

pub fn init(_url: Url, orders: &mut impl Orders<Msg>) -> Model {
    Model {}
}

pub fn update(msg: Msg, mut model: &mut Model, orders: &mut impl Orders<Msg>) {
    log!("Library update", msg);
}

pub fn view(model: &Model) -> Node<Msg> {
    nav![
        C!["panel"],
        // p![C!["panel-heading"], "Music Library"],
        div![
            C!["panel-block"],
            p![
                C!["control", "has-icons-left"],
                input![C!["input"]],
                span![C!["icon", "is-left"], i![C!["fas", "fa-search"]]]
            ],
        ],
        p![
            C!["panel-tabs"],
            a![C!["is-active"], "Files"],
            a!["Saved slaylists"],
            a!["Dynamic playlists"],
            a!["Radio stations"],
        ],
        a![
            C!["panel-block", "is-active"],
            span![C!["panel-icon"], i![C!["fas", "fa-book"]]],
            "First item"
        ],
        a![
            C!["panel-block"],
            span![C!["panel-icon"], i![C!["fas", "fa-code-branch"]]],
            "Second item"
        ],
    ]
}
