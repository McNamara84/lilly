FROM node:26.7.0-alpine AS dependencies

WORKDIR /app
RUN npm install --global npm@12.0.2
COPY package*.json ./
RUN npm ci

FROM dependencies AS build

COPY . .
RUN npm run build

FROM dependencies AS production-dependencies

RUN npm prune --omit=dev

FROM node:26.7.0-alpine AS production

WORKDIR /app
COPY --from=build /app/build ./build
COPY --from=dependencies /app/package.json ./
COPY --from=dependencies /app/package-lock.json ./
COPY --from=production-dependencies /app/node_modules ./node_modules

ENV PORT=3000
ENV HOST=0.0.0.0
EXPOSE 3000

CMD ["node", "build"]
