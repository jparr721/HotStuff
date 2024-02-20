import sys

SERVICE = R"""
  node{}:
   image: hotstuff
   restart: always
   ports:
    - "{}:2000"
   networks:
    stuffnet:
     ipv4_address: 172.20.0.{}
"""

NETWORK = R"""
networks:
  stuffnet:
   driver: bridge
   ipam:
    driver: default
    config:
      - subnet: 172.20.0.0/16
"""


def emit_docker_compose_file(n_nodes: int):
    with open("docker-compose.yml", "w") as f:
        f.write('version: "3"\n')
        f.write("services:")
        for i in range(n_nodes):
            f.write(
                SERVICE.format(
                    i,
                    2000 + i,
                    i + 2,
                )
            )
        f.write(NETWORK)
        f.write("\n")


if __name__ == "__main__":
    try:
        n = int(sys.argv[1])
    except:  # noqa: E722
        print("usage: python ./scripts/make_network <number_of_nodes>")
        exit(1)

    emit_docker_compose_file(n)
