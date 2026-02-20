.PHONY: verify init

init:
	git init
	git add .
	git commit -m "chore: project init"
	git remote add origin https://github.com/YASSERRMD/barq-coder.git
	git push -u origin main

verify:
	cargo fmt --check
	cargo clippy -- -D warnings
	cargo test

commit:
	@read -p "Commit message: " MSG && git add . && git commit -m "$$MSG" && git push
