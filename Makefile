SHELL := /bin/bash

.PHONY: bootstrap dev build check lint fmt test smoke-phase1 smoke-upload logs clean

bootstrap:
	cp -n .env.example .env || true
	cd frontend && npm install

dev:
	docker compose -f docker-compose.yml -f docker-compose.dev.yml up --build

stop:
	docker compose -f docker-compose.yml -f docker-compose.dev.yml stop

restart:
	docker compose -f docker-compose.yml -f docker-compose.dev.yml restart

down:
	docker compose -f docker-compose.yml -f docker-compose.dev.yml down

build:
	docker compose build

check:
	cargo check --workspace
	cd frontend && npm run check
	cd frontend && npm run build

lint:
	cargo clippy --workspace --all-targets -- -D warnings
	cd frontend && npm run lint

fmt:
	cargo fmt --all
	cd frontend && npm run format

test:
	cargo test --workspace

smoke-phase1:
	bash scripts/phase1_smoke.sh

smoke-upload:
	bash scripts/upload_smoke.sh

logs:
	docker compose logs -f api worker frontend

clean:
	docker compose -f docker-compose.yml -f docker-compose.dev.yml down -v --remove-orphans
