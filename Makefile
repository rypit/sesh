# sesh developer commands. `make check` runs everything that needs no
# running stack; `make stack-test` starts the stack with the stub model
# server and runs the stack tests (needs Docker).

COMPOSE_FILES := -f deploy/compose.yaml -f deploy/compose.stub.yaml

.PHONY: check fmt clippy test deny diagram compose-config no-instance-data stack-test

check: fmt clippy test deny diagram compose-config no-instance-data

fmt:
	cargo fmt --all --check

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

test:
	cargo test --workspace

deny:
	cargo deny check

diagram:
	python3 scripts/render-architecture.py --check

compose-config:
	mkdir -p target/compose-check
	touch target/compose-check/litellm.yaml
	printf '%s\n' POSTGRES_PASSWORD=x REDIS_PASSWORD=x LITELLM_MASTER_KEY=x WEBUI_SECRET_KEY=x \
	  WEBUI_ADMIN_EMAIL=x WEBUI_ADMIN_PASSWORD=x GATEWAY_DB_PASSWORD=x SEARXNG_SECRET=x > target/compose-check/secrets.env
	SESH_DATA_DIR=$(CURDIR)/target/compose-check SESH_WEBUI_PORT=3000 SESH_ROUTER_PORT=4000 \
	  docker compose --env-file target/compose-check/secrets.env $(COMPOSE_FILES) config -q
	SESH_EVAL_DIR=$(CURDIR)/target/compose-check docker compose -f deploy/eval/compose.yaml config -q

# Fails if instance data (secrets or rendered config) is ever committed (ADR 0003).
no-instance-data:
	@if git ls-files | grep -E '(^|/)(secrets\.env|litellm\.yaml|sesh\.toml)$$'; then \
	  echo "instance data is tracked in git; remove it" >&2; exit 1; fi

stack-test:
	cargo build --workspace
	cargo test -p sesh-cli --test stack -- --ignored --test-threads=1
