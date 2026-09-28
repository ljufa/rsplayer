# Vendored docs site assets

Served from the docs site itself so visitors' browsers make no requests to third-party CDNs.

- `docsify-*`: docsify 4.13.1 (`https://cdn.jsdelivr.net/npm/docsify@4.13.1/lib/...`), MIT license.
  `docsify-vue.css` has its Google Fonts `@import` removed.
- `prism-*.min.js`: Prism 1.29.0 language components (`https://cdn.jsdelivr.net/npm/prismjs@1.29.0/components/...`), MIT license.

To upgrade, download the new versions from the same paths and remove the `@import` from the theme again.
