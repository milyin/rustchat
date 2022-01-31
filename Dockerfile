FROM rust:latest
WORKDIR /rustchat
ADD . .
EXPOSE 80
CMD hostname -I; ./chat
