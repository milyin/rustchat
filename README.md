# rustchat
Azure webapp deploy experimenent. Code is cloned from https://github.com/SergioBenitez/Rocket/tree/master/examples/chat

Build docker container - use "update_builder.cmd" - for prearing local container "rust_builder" with fetched crates and "build.cmd" - 
for build "rustchat" container from sources. Rerun update_builder.cmd each time when build.cmd itself spend time on repeated fetching
of updated crates.
TODO: get rid of update_builder.cmd and store cargo's cache locally instead of "rust_builder" container. Haven't found how to do this yet.

Run docker container locally, access site on localhost:8000. Use '-it' is optional - it adds Ctrl-Break support and color 
display on windows terminal
```
docker run -it -p 8000:80 rustchat
```

Access command line in docker container
```
docker run -it -p 8000:80 rustchat /bin/sh
```

Create Azure resource group
```
az group create -n rustchat -l eastus2
```

Create Azure container registry
```
az acr create --resource-group rustchat --name rustchat --sku basic

```

Upload docker container to Azure container registry
```
docker tag rustchat rustchat.azurecr.io/rustchat
docker push rustchat.azurecr.io/rustchat

```

Create Azure webapp
```
az appservice plan create -n rustchat -g rustchat --is-linux --location eastus2 --sku F1 
az webapp create -n rustchat --plan rustchat -g rustchat --deployment-container-image-name rustchat.azurecr.io/rustchat
```

Provide webapp credentials to pull image from container registry 
(from https://docs.microsoft.com/en-us/azure/app-service/configure-custom-container?pivots=container-windows)

Enable system-assigned managed identity to webapp and show ID of this identity
```
az webapp identity assign -g rustchat -n rustchat --query principalId
```
Result is like this (1):
```
"deadbeef-1234-5678-9abc-def0123456"
```

Show resource id of container registry
```
az acr show -g rustchat -n rustchat --query id
```
Result is like this (2):
```
"/subscriptions/defec8ed-0123-4567-8901-23456789abcd/resourceGroups/rustchat/providers/Microsoft.ContainerRegistry/registries/rustchat"
```

Grant image pull permission in container registry with identity (2) to webapp with identity (1)
```
az role assignment create --role "AcrPull" --assignee "deadbeef-1234-5678-9abc-def0123456"
  --scope "/subscriptions/defec8ed-0123-4567-8901-23456789abcd/resourceGroups/rustchat/providers/Microsoft.ContainerRegistry/registries/rustchat"
```

Allow webapp to pull image using managed identity
```
az webapp config set -g rustchat -n rustchat --generic-configurations "{\"acrUseManagedIdentityCreds\": true}"
```

Show webapp in browser. Wait several minutes to finish pulling docker image
```
az webapp browse -n rustchat -g rustchat
```

Download logs for diagnostic
```
az webapp log download -n rustchat -g rustchat
```
