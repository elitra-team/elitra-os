CARGO   = cargo +nightly

ZFLAGS = -Z build-std=core
CARGOFLAGS = $(ZFLAGS) --target x86_64-unknown-none --release

.PHONY: all clean

BINS = hello cat ls touch mkdir rm rmdir mv cp true false echo yes seq sleep clear uname basename dirname pwd which tee env init ping halt reboot shutdown ln date chmod id whoami head tail wc find grep df du diff sort tr cut xargs strace mount umount lsof kill free nproc uptime test_cmd procinfo

all:
	$(CARGO) build $(CARGOFLAGS) $(addprefix --bin ,$(BINS))
	for bin in $(BINS); do cp target/x86_64-unknown-none/release/$$bin apps/$$bin.elf; done
	@echo "  Copied: $(addsuffix .elf,$(BINS))"

clean:
	$(CARGO) clean
	rm -f apps/*.elf
