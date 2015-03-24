FROM schickling/rust:latest

RUN apt-get update
RUN apt-get install -y libcurl4-openssl-dev

CMD /bin/bash
