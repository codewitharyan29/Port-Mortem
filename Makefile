.PHONY: build test adapter-test fuzz cli-test mutation verify

build:
	cargo build --release

test: build
	cargo test --release

adapter-test: build
	PYTHONPATH=adapter python3 -m pytest -q

fuzz: build
	python3 fuzz/harness.py 3000

cli-test: build
	python3 cli_difftest.py 200

mutation: build
	python3 mutation_test.py

verify: test adapter-test fuzz cli-test mutation
	@echo "All verification layers passed."
