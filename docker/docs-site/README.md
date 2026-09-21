# docs.rsplayer.de

The docsify site in `docs/` served by nginx in a container behind Traefik on the VPS
(same pattern as the personal site). Nothing is built: docsify renders in the browser
(its JS is loaded from jsdelivr, see `docs/index.html`).

## Local test

From the repo root:

```
docker build -f docker/docs-site/Dockerfile -t rsplayer-docs . && docker run --rm -p 8080:80 rsplayer-docs
```

Then open http://localhost:8080.

