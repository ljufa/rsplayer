# Docsify site is static: just serve docs/ with nginx. Build from the repo root:
#   docker build -f docker/docs-site/Dockerfile .
FROM nginx:alpine
COPY docker/docs-site/nginx.conf /etc/nginx/conf.d/default.conf
COPY docs/ /usr/share/nginx/html/
