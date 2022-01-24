FROM mcr.microsoft.com/windows/nanoserver:20H2
ADD target/release/chat.exe /
ADD static /static
ADD Rocket.toml /
EXPOSE 80
CMD chat.exe
