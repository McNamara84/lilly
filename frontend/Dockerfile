FROM node:26.13.0-alpine@sha256:c6ad9e9c1d80f6eb607c26b4b0a132ff7a2344fad1c78ea3baa2a7d4c8de6d94 AS dependencies

WORKDIR /app
RUN npm install --global npm@12.2.1
COPY package*.json ./
RUN npm ci

FROM dependencies AS build

COPY . .
RUN npm run build

FROM dependencies AS production-dependencies

RUN npm ci --omit=dev --ignore-scripts --offline --no-audit

FROM node:26.13.0-alpine@sha256:c6ad9e9c1d80f6eb607c26b4b0a132ff7a2344fad1c78ea3baa2a7d4c8de6d94 AS production

# Package managers are only needed during the build; omit their bundled dependencies at runtime.
RUN rm -rf /usr/local/lib/node_modules/npm /opt/yarn-* && \
    rm -f /usr/local/bin/npm /usr/local/bin/npx /usr/local/bin/yarn /usr/local/bin/yarnpkg

WORKDIR /app
COPY --from=build /app/build ./build
COPY --from=dependencies /app/package.json ./
COPY --from=dependencies /app/package-lock.json ./
COPY --from=production-dependencies /app/node_modules ./node_modules

ENV PORT=3000
ENV HOST=0.0.0.0
EXPOSE 3000

CMD ["node", "build"]
