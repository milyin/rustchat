# rustchat
Azure webapp deploy experimenent

Build docker container
```
docker build -f Dockerfile.build -t rustchat_build .
docker build -f Dockerfile.configure -t rustchat .
```

Run docker container locally, access site on localhost:8000. Use '-it' is optional - it adds Ctrl-Break support and color 
display on windows terminal
```
docker run -it -p 80:8000 rustchat
```

Access command line in docker container
```
docker run -it -p 80:8000 rustchat /bin/sh
```

Create Azure resources for application
```
az group create -n rustchat -l eastus2
az acr create --resource-group rustchat --name rustchat --sku basic

```
