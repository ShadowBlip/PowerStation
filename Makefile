ALL_RS := $(shell find src -name '*.rs')
PREFIX ?= /usr
CACHE_DIR := .cache

# Docker image variables
IMAGE_NAME ?= rust-cmake
IMAGE_TAG ?= latest

##@ General

# The help target prints out all targets with their descriptions organized
# beneath their categories. The categories are represented by '##@' and the
# target descriptions by '##'. The awk commands is responsible for reading the
# entire set of makefiles included in this invocation, looking for lines of the
# file as xyz: ## something, and then pretty-format the target and help. Then,
# if there's a line with ##@ something, that gets pretty-printed as a category.
# More info on the usage of ANSI control characters for terminal formatting:
# https://en.wikipedia.org/wiki/ANSI_escape_code#SGR_parameters
# More info on the awk command:
# http://linuxcommand.org/lc3_adv_awk.php

.PHONY: help
help: ## Display this help.
	@awk 'BEGIN {FS = ":.*##"; printf "\nUsage:\n  make \033[36m<target>\033[0m\n"} /^[a-zA-Z_0-9-]+:.*?##/ { printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)

.PHONY: install
install: build ## Install PowerStation to the given prefix (default: PREFIX=/usr)
	install -D -m 755 target/release/powerstation \
		$(PREFIX)/bin/powerstation
	install -D -m 644 rootfs/usr/share/dbus-1/system.d/org.shadowblip.PowerStation.conf \
		$(PREFIX)/share/dbus-1/system.d/org.shadowblip.PowerStation.conf
	install -D -m 644 rootfs/usr/lib/systemd/system/powerstation.service \
		$(PREFIX)/lib/systemd/system/powerstation.service
ifndef NO_RELOAD
	systemctl reload dbus
endif

.PHONY: uninstall
uninstall: ## Uninstall PowerStation
	rm $(PREFIX)/bin/powerstation
	rm $(PREFIX)/share/dbus-1/system.d/org.shadowblip.PowerStation.conf
	rm $(PREFIX)/lib/systemd/system/powerstation.service

##@ Development

.PHONY: debug
debug: target/debug/powerstation  ## Build debug build
target/debug/powerstation: $(ALL_RS) Cargo.lock
	cargo build

.PHONY: build
build: target/release/powerstation ## Build release build
target/release/powerstation: $(ALL_RS) Cargo.lock
	cargo build --release

.PHONY: all
all: build debug ## Build release and debug builds

.PHONY: run
run: setup debug ## Build and run
	sudo ./target/debug/powerstation

.PHONY: clean
clean: ## Remove build artifacts
	rm -rf target
	rm -rf .cache
	rm -rf dist

.PHONY: format
format: ## Run rustfmt on all source files
	rustfmt --edition 2021 $(ALL_RS)

.PHONY: setup
setup: /usr/share/dbus-1/system.d/org.shadowblip.PowerStation.conf ## Install dbus policies
/usr/share/dbus-1/system.d/org.shadowblip.P$(CACHE_DIR)/owerStation.conf:
	sudo ln $(PWD)/rootfs/usr/share/dbus-1/system.d/org.shadowblip.PowerStation.conf \
		/usr/share/dbus-1/system.d/org.shadowblip.PowerStation.conf
	sudo systemctl reload dbus

##@ Distribution

.PHONY: dist
dist: dist/powerstation.tar.gz ## Build a redistributable archive of the project
dist/powerstation.tar.gz: build
	rm -rf $(CACHE_DIR)/powerstation
	mkdir -p $(CACHE_DIR)/powerstation
	$(MAKE) install PREFIX=$(CACHE_DIR)/powerstation/usr NO_RELOAD=true
	mkdir -p dist
	tar cvfz $@ -C $(CACHE_DIR) powerstation

# Refer to .releaserc.yaml for release configuration
.PHONY: sem-release 
sem-release: ## Publish a release with semantic release 
	npx semantic-release

# Build the docker container for running in docker
.PHONY: docker-builder
docker-builder:
	docker build -t $(IMAGE_NAME):$(IMAGE_TAG) .

# E.g. make in-docker TARGET=build
.PHONY: in-docker
in-docker: docker-builder
	@# Run the given make target inside Docker
	docker run --rm \
		-v $(PWD):/src \
		--workdir /src \
		-e HOME=/home/build \
		--user $(shell id -u):$(shell id -g) \
		$(IMAGE_NAME):$(IMAGE_TAG) \
		make $(TARGET)

