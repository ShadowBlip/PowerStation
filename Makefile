ALL_RS := $(shell find src -name '*.rs')
PREFIX ?= /usr

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
install: build ## Install LightningBus to the given prefix (default: PREFIX=/usr)
	install -D -m 755 target/release/lightning-bus \
		$(PREFIX)/bin/lightning-bus
	install -D -m 644 rootfs/usr/share/dbus-1/system.d/org.shadowblip.LightningBus.conf \
		$(PREFIX)/share/dbus-1/system.d/org.shadowblip.LightningBus.conf
	install -D -m 644 rootfs/usr/lib/systemd/system/lightningbus.service \
		$(PREFIX)/lib/systemd/system/lightningbus.service
	systemctl reload dbus

.PHONY: uninstall
uninstall: ## Uninstall LightningBus
	rm $(PREFIX)/bin/lightning-bus
	rm $(PREFIX)/share/dbus-1/system.d/org.shadowblip.LightningBus.conf
	rm $(PREFIX)/lib/systemd/system/lightningbus.service

##@ Development

.PHONY: debug
debug: target/debug/lightning-bus  ## Build debug build
target/debug/lightning-bus: $(ALL_RS) Cargo.lock
	cargo build

.PHONY: build
build: target/release/lightning-bus ## Build release build
target/release/lightning-bus: $(ALL_RS) Cargo.lock
	cargo build --release

.PHONY: all
all: build debug ## Build release and debug builds

.PHONY: run
run: setup debug ## Build and run
	sudo ./target/debug/lightning-bus

.PHONY: clean
clean: ## Remove build artifacts
	rm -rf target

.PHONY: format
format: ## Run rustfmt on all source files
	rustfmt --edition 2021 $(ALL_RS)

.PHONY: setup
setup: /usr/share/dbus-1/system.d/org.shadowblip.LightningBus.conf ## Install dbus policies
/usr/share/dbus-1/system.d/org.shadowblip.LightningBus.conf:
	sudo ln $(PWD)/rootfs/usr/share/dbus-1/system.d/org.shadowblip.LightningBus.conf \
		/usr/share/dbus-1/system.d/org.shadowblip.LightningBus.conf
	sudo systemctl reload dbus
