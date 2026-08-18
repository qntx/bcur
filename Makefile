# Delegates to the Justfile so fmt/clippy flags stay in one place.

.PHONY: all build check check-no-std test clippy clippy-fix fmt doc update quality

all:
	just all

build:
	just build

check:
	just check

check-no-std:
	just check-no-std

update:
	just update

test:
	just test

clippy:
	just clippy

clippy-fix:
	just clippy-fix

fmt:
	just fmt

doc:
	just doc

quality:
	just quality
