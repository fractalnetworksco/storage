CARGO=cargo
DOCKER=docker
IMAGE_NAME=registry.gitlab.com/fractalnetworks/storage
IMAGE_TAG=latest
STORAGE_DATABASE=/var/tmp/gateway.db
STORAGE_PATH=/var/tmp/storage
STORAGE_ADDRESS=127.0.0.1
STORAGE_PORT=8000
ARCH=amd64

release:
	$(CARGO) build --release

debug:
	$(CARGO) build

doc:
	$(CARGO) doc

test:
	$(CARGO) test

run: release
	@mkdir -p $(STORAGE_PATH)
	@touch $(STORAGE_DATABASE)
	RUST_LOG=info,sqlx=warn RUST_BACKTRACE=1 ROCKET_ADDRESS=$(STORAGE_ADDRESS) ROCKET_PORT=$(STORAGE_PORT) $(CARGO) run --release -- --database $(STORAGE_DATABASE) --storage $(STORAGE_PATH)

docker:
	$(DOCKER) build . -t $(IMAGE_NAME):$(IMAGE_TAG)

docker-push:
	$(DOCKER) push $(IMAGE_NAME):$(IMAGE_TAG)

docker-run:
	-$(DOCKER) network create fractal
	$(DOCKER) run --network fractal --name gateway -it --privileged --rm -p 8000:8000 $(IMAGE_NAME):$(IMAGE_TAG)

get-release-artifact:
	./scripts/get-release-artifact.sh $(ARCH)
