# Zestins: ZestBay's own LV2 plugins, built into one bundle, zestins.lv2:
#
#   zestins.so      every plugin's DSP (dsp/*.c, descriptors in zestins.c)
#   zestins_ui.so   one X11 UI for all of them (ui/ui.c)
#   *.ttl           generated from spec.c by ttlgen
#
#   make                      build into build/zestins.lv2
#   make install              copy the bundle to ~/.lv2
#   make install DESTDIR=$pkgdir LV2_DIR=/usr/lib/lv2   (packaging)
#   make uninstall / make clean

BUILD    ?= build
LV2_DIR  ?= $(HOME)/.lv2
DESTDIR  ?=
CC       ?= cc
CFLAGS   ?= -O2 -g
# Always applied, also when a packager passes CFLAGS on the command line.
ZFLAGS   := -std=c11 -D_DEFAULT_SOURCE -Wall -Wextra -fPIC -fvisibility=hidden
LV2_CFLAGS := $(shell pkg-config --cflags lv2 2>/dev/null)
X11_CFLAGS := $(shell pkg-config --cflags x11 2>/dev/null)
X11_LIBS   := $(shell pkg-config --libs x11 2>/dev/null || echo -lX11)

BUNDLE   := $(BUILD)/zestins.lv2
HEADERS  := spec.h dsp/dsp.h
DSP_SRC  := zestins.c $(wildcard dsp/*.c)

all: $(BUNDLE)/zestins.so $(BUNDLE)/zestins_ui.so $(BUNDLE)/manifest.ttl

$(BUNDLE):
	mkdir -p $@

$(BUNDLE)/zestins.so: $(DSP_SRC) $(HEADERS) | $(BUNDLE)
	$(CC) $(ZFLAGS) $(CFLAGS) $(LV2_CFLAGS) $(LDFLAGS) -shared $(DSP_SRC) -o $@ -lm

$(BUNDLE)/zestins_ui.so: ui/ui.c spec.c spec.h | $(BUNDLE)
	$(CC) $(ZFLAGS) $(CFLAGS) $(LV2_CFLAGS) $(X11_CFLAGS) $(LDFLAGS) -shared ui/ui.c spec.c -o $@ \
		$(X11_LIBS) -lm

$(BUILD)/ttlgen: ttlgen.c spec.c spec.h | $(BUNDLE)
	$(CC) -std=c11 -O1 -Wall -Wextra ttlgen.c spec.c -o $@

$(BUNDLE)/manifest.ttl: $(BUILD)/ttlgen
	rm -f $(BUNDLE)/*.ttl
	$(BUILD)/ttlgen $(BUNDLE)

install: all
	install -d $(DESTDIR)$(LV2_DIR)/zestins.lv2
	install -m644 $(BUNDLE)/*.ttl $(DESTDIR)$(LV2_DIR)/zestins.lv2/
	install -m755 $(BUNDLE)/*.so $(DESTDIR)$(LV2_DIR)/zestins.lv2/

uninstall:
	rm -rf $(DESTDIR)$(LV2_DIR)/zestins.lv2

clean:
	rm -rf $(BUILD)

.PHONY: all install uninstall clean
