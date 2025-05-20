NAME := $(shell grep 'name =' Cargo.toml | head -n 1 | cut -d'"' -f2)
VERSION := $(shell grep '^version =' Cargo.toml | cut -d'"' -f2)
TARGET_ARCH ?= $(shell rustc -vV | sed -n 's/host: //p')
ARCH := $(shell echo "$(TARGET_ARCH)" | cut -d'-' -f1)
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
	install -D -m 755 target/$(TARGET_ARCH)/release/powerstation \
		$(PREFIX)/bin/powerstation
	install -D -m 644 rootfs/usr/share/dbus-1/system.d/org.shadowblip.PowerStation.conf \
		$(PREFIX)/share/dbus-1/system.d/org.shadowblip.PowerStation.conf
	install -D -m 644 rootfs/usr/lib/systemd/system/powerstation.service \
		$(PREFIX)/lib/systemd/system/powerstation.service
	install -D -m644 -t $(PREFIX)/share/powerstation/platform/ \
		rootfs/usr/share/powerstation/platform/*
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
debug: target/$(TARGET_ARCH)/debug/powerstation  ## Build debug build
target/$(TARGET_ARCH)/debug/powerstation: $(ALL_RS) Cargo.lock
	cargo build --target $(TARGET_ARCH)

.PHONY: build
build: target/$(TARGET_ARCH)/release/powerstation ## Build release build
target/$(TARGET_ARCH)/release/powerstation: $(ALL_RS) Cargo.lock
	cargo build --release --target $(TARGET_ARCH)

.PHONY: all
all: build debug ## Build release and debug builds

.PHONY: run
run: debug ## Build and run
	sudo ./target/$(TARGET_ARCH)/debug/powerstation

.PHONY: test
test: debug ## Build and run all tests
	cargo test -- --show-output

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
dist: dist/$(NAME)-$(ARCH).tar.gz dist/$(NAME)-$(VERSION)-1.$(ARCH).rpm ## Create all redistributable versions of the project

.PHONY: dist-archive
dist-archive: dist/powerstation-$(ARCH).tar.gz ## Build a redistributable archive of the project
dist/powerstation-$(ARCH).tar.gz: build
	rm -rf $(CACHE_DIR)/powerstation
	mkdir -p $(CACHE_DIR)/powerstation
	$(MAKE) install PREFIX=$(CACHE_DIR)/powerstation/usr NO_RELOAD=true
	mkdir -p dist
	tar cvfz $@ -C $(CACHE_DIR) powerstation
	cd dist && sha256sum powerstation-$(ARCH).tar.gz > powerstation-$(ARCH).tar.gz.sha256.txt

.PHONY: dist-rpm
dist-rpm: dist/$(NAME)-$(VERSION)-1.$(ARCH).rpm ## Build a redistributable RPM package
dist/$(NAME)-$(VERSION)-1.$(ARCH).rpm: target/$(TARGET_ARCH)/release/$(NAME)
	mkdir -p dist
	cargo install cargo-generate-rpm
	cargo generate-rpm --target $(TARGET_ARCH)
	cp ./target/$(TARGET_ARCH)/generate-rpm/$(NAME)-$(VERSION)-1.$(ARCH).rpm dist
	cd dist && sha256sum $(NAME)-$(VERSION)-1.$(ARCH).rpm > $(NAME)-$(VERSION)-1.$(ARCH).rpm.sha256.txt

INTROSPECT_CARD ?= Card2
INTROSPECT_CONNECTOR ?= eDP/1
.PHONY: introspect
introspect: ## Generate DBus XML
	echo "Generating DBus XML spec..."
	mkdir -p bindings/dbus-xml
	busctl introspect org.shadowblip.PowerStation \
		/org/shadowblip/Performance/CPU --xml-interface > bindings/dbus-xml/org-shadowblip-cpu.xml
	xmlstarlet ed -L -d '//node[@name]' bindings/dbus-xml/org-shadowblip-cpu.xml
	busctl introspect org.shadowblip.PowerStation \
		/org/shadowblip/Performance/CPU/Core0 --xml-interface > bindings/dbus-xml/org-shadowblip-cpu-core.xml
	busctl introspect org.shadowblip.PowerStation \
		/org/shadowblip/Performance/GPU --xml-interface > bindings/dbus-xml/org-shadowblip-gpu.xml
	xmlstarlet ed -L -d '//node[@name]' bindings/dbus-xml/org-shadowblip-gpu.xml
	busctl introspect org.shadowblip.PowerStation \
		/org/shadowblip/Performance/GPU/$(INTROSPECT_CARD) --xml-interface > bindings/dbus-xml/org-shadowblip-gpu-card.xml
	xmlstarlet ed -L -d '//node[@name]' bindings/dbus-xml/org-shadowblip-gpu-card.xml
	busctl introspect org.shadowblip.PowerStation \
		/org/shadowblip/Performance/GPU/Card2/$(INTROSPECT_CONNECTOR) --xml-interface > bindings/dbus-xml/org-shadowblip-gpu-card-connector.xml

XSL_TEMPLATE := ./docs/dbus2markdown.xsl
.PHONY: docs
docs: ## Generate markdown docs for DBus interfaces
	mkdir -p docs
	xsltproc --novalid -o docs/cpu.md $(XSL_TEMPLATE) bindings/dbus-xml/org-shadowblip-cpu.xml
	mdformat ./docs/cpu.md
	sed -i 's/DBus Interface API/CPU DBus Interface API/g' ./docs/cpu.md
	xsltproc --novalid -o docs/cpu-core.md $(XSL_TEMPLATE) bindings/dbus-xml/org-shadowblip-cpu-core.xml
	mdformat ./docs/cpu-core.md
	sed -i 's/DBus Interface API/CPU.Core DBus Interface API/g' ./docs/cpu-core.md
	xsltproc --novalid -o docs/gpu.md $(XSL_TEMPLATE) bindings/dbus-xml/org-shadowblip-gpu.xml
	mdformat ./docs/gpu.md
	sed -i 's/DBus Interface API/GPU DBus Interface API/g' ./docs/gpu.md
	xsltproc --novalid -o docs/gpu-card.md $(XSL_TEMPLATE) bindings/dbus-xml/org-shadowblip-gpu-card.xml
	mdformat ./docs/gpu-card.md
	sed -i 's/DBus Interface API/GPU.Card DBus Interface API/g' ./docs/gpu-card.md
	xsltproc --novalid -o docs/gpu-card-connector.md $(XSL_TEMPLATE) bindings/dbus-xml/org-shadowblip-gpu-card-connector.xml
	mdformat ./docs/gpu-card-connector.md
	sed -i 's/DBus Interface API/GPU.Card.Connector DBus Interface API/g' ./docs/gpu-card-connector.md

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
		-e TARGET_ARCH=$(TARGET_ARCH) \
		-e PKG_CONFIG_SYSROOT_DIR="/usr/$(ARCH)-linux-gnu" \
		--user $(shell id -u):$(shell id -g) \
		$(IMAGE_NAME):$(IMAGE_TAG) \
		make $(TARGET)

