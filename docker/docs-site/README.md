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

## Deploy (VPS)

DNS: `A`/`AAAA` record `docs.rsplayer.de` to the VPS. Traefik (already running there) handles routing
and the certificate through the labels in `docker-compose.yaml`.

### Automatic (GitHub Actions self-hosted runner)

`.github/workflows/docs-deploy.yml` runs on every push to the public `main` that touches `docs/**`
or `docker/docs-site/**` (or manually via "Run workflow"). It runs on the VPS itself, on the
self-hosted runner labelled `rsplayer-docs`, checks out `docs/` and `docker/docs-site/` and runs
`docker-compose up -d --build` locally, then smoke-tests the public URL. No SSH, keys or secrets
are involved, and the private `origin` remote is not part of it.

Runner requirements: label `rsplayer-docs`, its user can run docker/docker-compose, and the
external `proxy` docker network exists. The host and cert resolver are hardcoded in `docker-compose.yaml`.

### Manual

On the VPS, in a clone or copy of the repo: `cd docker/docs-site && docker-compose up -d --build`.
