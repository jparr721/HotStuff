.PHONY: dev run test docker_build docker docker_all

dev:
	cargo run
run:
	cargo run --release
test:
	cargo test
docker_build:
	docker build --rm -f docker/node.Dockerfile -t hotstuff .
docker:
	docker-compose up -d
docker_all:
	$(MAKE) docker_build
	$(MAKE) docker