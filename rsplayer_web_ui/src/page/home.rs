use crate::Urls;
use seed::{a, attrs, div, h1, h2, prelude::*, section, strong, C};

pub fn view<Ms>(base_url: &Url) -> Node<Ms> {
    section![C!["hero", "is-medium", "ml-6"],
        div![C!["hero-body"],
            h1![C!["title", "is-size-1"],
                "Welcome to RSPlayer UI",
            ],
            a![attrs!{At::Href => "https://seed-rs.org/"},
                h2![C!["subtitle", "is-size-3"],
                    "powered by seed-rs.org"
                ]
            ],
            a![C!["button", "is-dark", "mt-5", "is-size-5"], attrs!{At::Href => Urls::new(base_url).settings()},
                strong!["This is your first first visit, please go to Settings to complete configuration."],
            ],
        ],
    ]
}
