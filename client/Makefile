DOCKER=docker
CARGO=cargo
IPFS_PORT=34273

ipfs:
	$(DOCKER) run -it --rm -p $(IPFS_PORT):5001 ipfs/go-ipfs

test:
	IPFS_API=http://localhost:$(IPFS_PORT) $(CARGO) test -- --include-ignored
