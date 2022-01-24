# rustchat
Azure webapp deploy experimenent

Build docker container
```
cargo build
docker build -t rustchat .
```

Run docker container locally, access site on localhost:8000. Use '-it' is optional - it adds Ctrl-Break support and color 
display on windows terminal
```
docker run -it -p 80:8000 rustchat
```

Access command line in docker container
```
docker run -it -p 80:8000 rustchat cmd.exe
```
