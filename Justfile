_default:
	@just --list --unsorted --list-heading '' --list-prefix '—— '

# Run your current project
run:
	RUST_LOG='info,_=warn,rocket=warn,discord_smp_link=debug' cargo run

# Starts the docker compose file with the provided scope
up SCOPE:
	docker compose --file docker-compose.{{SCOPE}}.yml up -d
# Stops the docker compose file with the provided scope
down SCOPE:
	docker compose --file docker-compose.{{SCOPE}}.yml down
