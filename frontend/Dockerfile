FROM node:22-alpine AS base
WORKDIR /app
COPY frontend/package*.json ./
RUN npm install

FROM base AS build
COPY frontend/ .
RUN npm run build

FROM base AS dev
COPY frontend/ .
CMD ["npm", "run", "dev", "--", "--host", "0.0.0.0"]

FROM node:22-alpine AS runtime
WORKDIR /app
COPY --from=build /app .
EXPOSE 4173
CMD ["npm", "run", "preview", "--", "--host", "0.0.0.0"]
