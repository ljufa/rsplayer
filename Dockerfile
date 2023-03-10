FROM ubuntu:jammy

RUN apt update && apt install -y alsa-base alsa-utils
ADD rsplayer_amd64 /usr/bin/rsplayer
RUN chmod +x /usr/bin/rsplayer
EXPOSE 80
VOLUME [ "/opt/rsplayer" ]
WORKDIR /opt/rsplayer
CMD "/usr/bin/rsplayer"
