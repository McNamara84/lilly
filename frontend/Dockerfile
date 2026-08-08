FROM node:26.7.0-alpine AS build

WORKDIR /app
RUN npm install --global npm@12.0.2
COPY package*.json ./
RUN npm ci
COPY . .
RUN npm run build

FROM node:26.7.0-alpine AS production

WORKDIR /app
RUN npm install --global npm@12.0.2
COPY --from=build /app/build ./build
COPY --from=build /app/package.json ./
COPY --from=build /app/package-lock.json ./
RUN npm ci --omit=dev

ENV PORT=3000
ENV HOST=0.0.0.0
EXPOSE 3000

CMD ["node", "build"]
