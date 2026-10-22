FROM node:26.2.0-bookworm-slim@sha256:e89172f5e6154ba212269866bf3fbadbca8eb7901e10c0eccf08f2147bfae505 AS build
WORKDIR /build
COPY package.json package-lock.json ./
COPY web/admin/package.json ./web/admin/package.json
COPY web/theme/package.json ./web/theme/package.json
RUN npm ci --ignore-scripts
COPY web ./web
ARG APP
RUN test "$APP" = "admin" -o "$APP" = "theme"
RUN npm run build --workspace=@rustpress/$APP && cp -r web/$APP/.output /output

FROM node:26.2.0-bookworm-slim@sha256:e89172f5e6154ba212269866bf3fbadbca8eb7901e10c0eccf08f2147bfae505
WORKDIR /app
COPY --from=build --chown=node:node /output ./
USER node
ENV NODE_ENV=production HOST=0.0.0.0 PORT=3000
EXPOSE 3000
CMD ["node", "server/index.mjs"]
