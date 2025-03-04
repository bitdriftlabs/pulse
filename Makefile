# Makefile: Container Release Builds
.PHONY: setup
setup:
	ARCH=${ARCH} ci/setup.sh

.PHONY: release-all
release-all: test
	SKIP_PROTO_GEN=1 cargo build --release --bin pulse-proxy
	SKIP_PROTO_GEN=1 cargo build --release --bin pulse-vrl-tester
	# Horrible hack to workaround the fact that you can't have a different .dockerignore per
	# invocation.
	cp -f Dockerfile.dockerignore .dockerignore

.PHONY: test
test: setup
	SKIP_PROTO_GEN=1 RUST_LOG=off,bd_panic=error RUST_BACKTRACE=1 cargo test --profile ci-test --workspace
	SKIP_PROTO_GEN=1 cargo build --profile ci-test --workspace

.PHONY: clippy
clippy: setup
	ci/check_license.sh
	ci/format.sh
	SKIP_PROTO_GEN=1 cargo clippy --profile ci-test --workspace --bins --examples --tests -- --no-deps

.PHONY: docker-build
docker-build:
	docker build --platform ${ARCH} -t public.ecr.aws/bitdrift/pulse:${IMAGE_TAG}-${TAG_SUFFIX} .

.PHONY: docker-push
docker-push:
	docker push public.ecr.aws/bitdrift/pulse:${IMAGE_TAG}-${TAG_SUFFIX}

.PHONY: docker-multi-arch-push
docker-multi-arch-push:
	docker manifest create public.ecr.aws/bitdrift/pulse:${IMAGE_TAG} \
		public.ecr.aws/bitdrift/pulse:${IMAGE_TAG}-amd64 \
		public.ecr.aws/bitdrift/pulse:${IMAGE_TAG}-arm64
	docker manifest push public.ecr.aws/bitdrift/pulse:${IMAGE_TAG}
