# One command builds and verifies this port end-to-end:
#   docker build -t natsort-rust . && docker run --rm natsort-rust
#
# Runs the full verification stack: native tests, natsort's original tests
# (unmodified, via the adapter), differential fuzz, CLI differential, and
# mutation testing. Everything needed is vendored in the repo (the original
# Python natsort source included), so no network access is required at run
# time beyond the initial image build.

FROM rust:1.83-slim

RUN apt-get update \
 && apt-get install -y --no-install-recommends python3 python3-pip make \
 && rm -rf /var/lib/apt/lists/* \
 && pip install --break-system-packages --no-cache-dir pytest hypothesis

WORKDIR /app
COPY . .

RUN cargo build --release

CMD ["sh", "-c", "\
  echo '== native rust tests ==' && cargo test --release -q && \
  echo '== natsort original tests vs the rust port ==' && \
  PYTHONPATH=adapter python3 -m pytest -q && \
  echo '== differential fuzz ==' && python3 fuzz/harness.py 2000 && \
  echo '== cli differential ==' && python3 cli_difftest.py 150 && \
  echo '== mutation testing ==' && python3 mutation_test.py \
"]
