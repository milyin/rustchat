docker run --rm -it -v "%cd%:/src" rust_builder /bin/bash -c "cd /src && cargo fetch && cargo build --offline --release --target-dir target_linux"
copy target_linux\release\chat .
docker build -t rustchat .
