CARGO=cargo
DOCKER=docker
IMAGE_NAME=registry.gitlab.com/fractalnetworks/storage
IMAGE_TAG=local
BUILD_TYPE=release
ARCH=amd64

default: target/$(BUILD_TYPE)/fractal-storage

target/release/fractal-storage:
	$(CARGO) build --release

target/debug/fractal-storage:
	$(CARGO) build

doc:
	$(CARGO) doc

test:
	$(CARGO) test

docker: target/$(BUILD_TYPE)/fractal-storage
	$(DOCKER) build . -t $(IMAGE_NAME):$(IMAGE_TAG)

docker-push:
	$(DOCKER) push $(IMAGE_NAME):$(IMAGE_TAG)

docker-run:
	-$(DOCKER) network create fractal
	$(DOCKER) run --network fractal --name storage -it --privileged --rm -p 8000:8000 $(IMAGE_NAME):$(IMAGE_TAG)

get-release-artifact:
	./scripts/get-release-artifact.sh $(ARCH)
