/*
 * wolf-pointer-inject: holds a Wayland virtual pointer open and injects events. See
 * tools/wolf-pointer-inject/README.md for why, the command grammar, acks and exit codes.
 */

#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include <wayland-client.h>

#include "wlr-virtual-pointer-unstable-v1-client-protocol.h"

/*
 * Button codes, defined here because <linux/input-event-codes.h> would add a
 * `linux-headers` dependency to the derivation for three integers.
 */
#define WOLF_BTN_LEFT 0x110   /* 272 */
#define WOLF_BTN_RIGHT 0x111  /* 273 */
#define WOLF_BTN_MIDDLE 0x112 /* 274 */

/* Highest zwlr_virtual_pointer_manager_v1 version this program understands.
 * Version 2 is what adds create_virtual_pointer_with_output. */
#define WOLF_MANAGER_MAX_VERSION 2

struct wolf_state {
	struct wl_display *display;
	struct wl_registry *registry;
	struct wl_seat *seat;
	struct wl_output *output;
	struct zwlr_virtual_pointer_manager_v1 *manager;
	struct zwlr_virtual_pointer_v1 *pointer;

	uint32_t manager_version;
	uint32_t extent_w;
	uint32_t extent_h;
	int output_count;
	int verbose;
};

static struct wolf_state state;

/* ---------------------------------------------------------------- output -- */

static void output_geometry(void *data, struct wl_output *wl_output, int32_t x, int32_t y,
			    int32_t physical_width, int32_t physical_height, int32_t subpixel,
			    const char *make, const char *model, int32_t transform)
{
	(void)data;
	(void)wl_output;
	(void)x;
	(void)y;
	(void)physical_width;
	(void)physical_height;
	(void)subpixel;
	(void)make;
	(void)model;
	(void)transform;
}

static void output_mode(void *data, struct wl_output *wl_output, uint32_t flags, int32_t width,
			int32_t height, int32_t refresh)
{
	struct wolf_state *st = data;
	(void)wl_output;
	(void)refresh;

	/* Only the CURRENT mode describes the pixel space motion_absolute is
	 * scaled against; a monitor can advertise many non-current modes. */
	if ((flags & WL_OUTPUT_MODE_CURRENT) == 0) {
		return;
	}
	if (width > 0 && height > 0) {
		st->extent_w = (uint32_t)width;
		st->extent_h = (uint32_t)height;
	}
}

static void output_done(void *data, struct wl_output *wl_output)
{
	(void)data;
	(void)wl_output;
}

static void output_scale(void *data, struct wl_output *wl_output, int32_t factor)
{
	(void)data;
	(void)wl_output;
	(void)factor;
}

static void output_name(void *data, struct wl_output *wl_output, const char *name)
{
	(void)data;
	(void)wl_output;
	(void)name;
}

static void output_description(void *data, struct wl_output *wl_output, const char *description)
{
	(void)data;
	(void)wl_output;
	(void)description;
}

static const struct wl_output_listener output_listener = {
	.geometry = output_geometry,
	.mode = output_mode,
	.done = output_done,
	.scale = output_scale,
	.name = output_name,
	.description = output_description,
};

/* -------------------------------------------------------------- registry -- */

static uint32_t min_u32(uint32_t a, uint32_t b)
{
	return a < b ? a : b;
}

static void registry_global(void *data, struct wl_registry *registry, uint32_t name,
			    const char *interface, uint32_t version)
{
	struct wolf_state *st = data;

	if (strcmp(interface, wl_seat_interface.name) == 0) {
		/* Version 1 is enough: nothing here listens for pointer events. */
		if (st->seat == NULL) {
			st->seat = wl_registry_bind(registry, name, &wl_seat_interface, 1);
		}
		return;
	}

	if (strcmp(interface, wl_output_interface.name) == 0) {
		st->output_count++;
		/* Version 2 suffices for wl_output.mode. Only the first output is bound; more
		 * than one is fatal below, because the coordinate model assumes exactly one. */
		if (st->output == NULL) {
			st->output = wl_registry_bind(registry, name, &wl_output_interface, 2);
			wl_output_add_listener(st->output, &output_listener, st);
		}
		return;
	}

	if (strcmp(interface, zwlr_virtual_pointer_manager_v1_interface.name) == 0) {
		uint32_t bind_version = min_u32(version, WOLF_MANAGER_MAX_VERSION);
		if (st->manager == NULL) {
			st->manager = wl_registry_bind(
				registry, name, &zwlr_virtual_pointer_manager_v1_interface,
				bind_version);
			st->manager_version = bind_version;
		}
		return;
	}
}

static void registry_global_remove(void *data, struct wl_registry *registry, uint32_t name)
{
	(void)data;
	(void)registry;
	(void)name;
}

static const struct wl_registry_listener registry_listener = {
	.global = registry_global,
	.global_remove = registry_global_remove,
};

/* ------------------------------------------------------------------ time -- */

/*
 * Real monotonic timestamps: GDK uses event times for click detection and ordering,
 * and a frozen clock risks GTK folding successive presses into one multi-press.
 */
static uint32_t time_ms(void)
{
	struct timespec ts;
	if (clock_gettime(CLOCK_MONOTONIC, &ts) != 0) {
		return 0;
	}
	uint64_t ms = (uint64_t)ts.tv_sec * 1000u + (uint64_t)(ts.tv_nsec / 1000000L);
	return (uint32_t)ms;
}

/* ---------------------------------------------------------------- parsing -- */

static int parse_long(const char *s, long *out)
{
	char *end = NULL;
	long v;

	errno = 0;
	v = strtol(s, &end, 10);
	if (end == s || end == NULL || *end != '\0' || errno == ERANGE) {
		return -1;
	}
	*out = v;
	return 0;
}

static int parse_double(const char *s, double *out)
{
	char *end = NULL;
	double v;

	errno = 0;
	v = strtod(s, &end);
	if (end == s || end == NULL || *end != '\0' || errno == ERANGE) {
		return -1;
	}
	*out = v;
	return 0;
}

/* ------------------------------------------------------------------ help -- */

static void print_help(FILE *out)
{
	fprintf(out,
		"wolf-pointer-inject — long-lived Wayland zwlr_virtual_pointer_v1 injector\n"
		"\n"
		"Usage: wolf-pointer-inject [-e WxH] [-v] [-h|--help]\n"
		"\n"
		"Options:\n"
		"  -e WxH        Override the motion extents (default: the single bound\n"
		"                output's current mode).\n"
		"  -v            Echo each received command to stderr.\n"
		"  -h, --help    Print this help and exit 0.\n"
		"\n"
		"Reads one command per line from stdin. Writes a readiness line, then one\n"
		"ack per command, to stdout (unbuffered).\n"
		"\n"
		"Commands:\n"
		"  motion <x> <y>            Absolute motion, in output pixels, followed by\n"
		"                            a frame. Both coordinates must lie INSIDE the\n"
		"                            resolved extents (0 <= x < W, 0 <= y < H);\n"
		"                            an out-of-range point is fatal (exit 10), never\n"
		"                            silently clamped.\n"
		"  press <button>            Button press (BTN_LEFT=272, BTN_RIGHT=273,\n"
		"                            BTN_MIDDLE=274), followed by a frame.\n"
		"  release <button>          Button release, followed by a frame.\n"
		"  axis <value>              Continuous vertical scroll (axis_source =\n"
		"                            continuous), followed by a frame.\n"
		"  axis <value> <discrete>   Discrete vertical scroll (axis_source = wheel,\n"
		"                            axis_discrete carries BOTH value and discrete),\n"
		"                            followed by a frame.\n"
		"  sync                      No events; a pure barrier (roundtrip + ack).\n"
		"  quit                      Destroy the pointer and exit 0.\n"
		"\n"
		"Protocol on stdout:\n"
		"  ready extents=<W>x<H> manager_version=<v> outputs=<n>\n"
		"  ok <seq>                  Command <seq> (1-based) was processed by the\n"
		"                            compositor (an ack follows a wl_display_roundtrip).\n"
		"  err <seq> <message>       Fatal; the process exits.\n"
		"\n"
		"Exit codes:\n"
		"  0  clean exit (quit, or EOF on stdin)\n"
		"  1  bad command-line usage\n"
		"  2  could not connect to the Wayland display (check WAYLAND_DISPLAY)\n"
		"  3  compositor does not advertise zwlr_virtual_pointer_manager_v1\n"
		"  4  compositor does not advertise wl_seat\n"
		"  5  no wl_output advertised and no -e WxH given\n"
		"  6  more than one wl_output (the coordinate model assumes exactly one)\n"
		"  7  resolved extents are zero in at least one axis\n"
		"  8  wl_display_roundtrip failed\n"
		"  9  unknown or malformed command\n"
		" 10  motion coordinate outside the resolved extents (0 <= x < W, 0 <= y < H)\n"
		"\n"
		"See tools/wolf-pointer-inject/README.md.\n");
}

/* ------------------------------------------------------------------ main -- */

static int roundtrip_or_die(int seq)
{
	if (wl_display_roundtrip(state.display) == -1) {
		fprintf(stdout, "err %d wayland roundtrip failed\n", seq);
		fprintf(stderr, "err %d wayland roundtrip failed\n", seq);
		exit(8);
	}
	return 0;
}

int main(int argc, char **argv)
{
	/*
	 * First statement: stdout to a pipe is block-buffered, so the readiness line and
	 * every ack would sit in libc's buffer and the scenario's poll would hang.
	 */
	setbuf(stdout, NULL);
	setbuf(stderr, NULL);

	char line[1024];
	long override_w = 0;
	long override_h = 0;
	int have_override = 0;
	int seq = 0;

	for (int i = 1; i < argc; i++) {
		if (strcmp(argv[i], "-h") == 0 || strcmp(argv[i], "--help") == 0) {
			print_help(stdout);
			return 0;
		}
		if (strcmp(argv[i], "-v") == 0) {
			state.verbose = 1;
			continue;
		}
		if (strcmp(argv[i], "-e") == 0) {
			if (i + 1 >= argc) {
				fprintf(stderr, "err -e needs an argument, e.g. -e 1280x720\n");
				return 1;
			}
			i++;
			if (sscanf(argv[i], "%ldx%ld", &override_w, &override_h) != 2 ||
			    override_w <= 0 || override_h <= 0) {
				fprintf(stderr, "err could not parse extents '%s' (want WxH)\n",
					argv[i]);
				return 1;
			}
			have_override = 1;
			continue;
		}
		fprintf(stderr, "err unknown option '%s'; try --help\n", argv[i]);
		return 1;
	}

	state.display = wl_display_connect(NULL);
	if (state.display == NULL) {
		fprintf(stderr,
			"err could not connect to a Wayland display; is WAYLAND_DISPLAY set "
			"and is the compositor running?\n");
		return 2;
	}

	state.registry = wl_display_get_registry(state.display);
	wl_registry_add_listener(state.registry, &registry_listener, &state);

	/* First roundtrip: the registry's globals arrive. */
	if (wl_display_roundtrip(state.display) == -1) {
		fprintf(stderr, "err wayland roundtrip failed while enumerating globals\n");
		return 8;
	}
	/* Second roundtrip: wl_output.mode (sent in response to the bind above)
	 * arrives, so extent_w/extent_h are populated. */
	if (wl_display_roundtrip(state.display) == -1) {
		fprintf(stderr, "err wayland roundtrip failed while reading the output mode\n");
		return 8;
	}

	if (state.manager == NULL) {
		fprintf(stderr,
			"err compositor does not advertise zwlr_virtual_pointer_manager_v1; "
			"pointer injection is impossible here\n");
		return 3;
	}
	if (state.seat == NULL) {
		fprintf(stderr, "err compositor does not advertise wl_seat\n");
		return 4;
	}
	if (state.output_count == 0 && !have_override) {
		fprintf(stderr,
			"err compositor advertised no wl_output and no -e WxH was given, so "
			"the motion extents cannot be resolved\n");
		return 5;
	}
	if (state.output_count > 1) {
		fprintf(stderr,
			"err compositor advertised %d wl_outputs; this tool's coordinate "
			"model assumes exactly one output, so absolute motion would be "
			"ambiguous\n",
			state.output_count);
		return 6;
	}

	if (have_override) {
		state.extent_w = (uint32_t)override_w;
		state.extent_h = (uint32_t)override_h;
	}
	if (state.extent_w == 0 || state.extent_h == 0) {
		fprintf(stderr,
			"err resolved motion extents are %ux%u; the compositor divides by "
			"them, so neither axis may be zero\n",
			state.extent_w, state.extent_h);
		return 7;
	}

	/*
	 * Prefer _with_output (manager v2+, one output): it pins motion_absolute's space
	 * to that output, so the extents are stated to be the output's pixel size.
	 */
	if (state.manager_version >= 2 && state.output != NULL) {
		state.pointer = zwlr_virtual_pointer_manager_v1_create_virtual_pointer_with_output(
			state.manager, state.seat, state.output);
	} else {
		state.pointer = zwlr_virtual_pointer_manager_v1_create_virtual_pointer(state.manager,
										      state.seat);
	}
	if (state.pointer == NULL) {
		fprintf(stderr, "err could not create a zwlr_virtual_pointer_v1\n");
		return 3;
	}
	if (wl_display_roundtrip(state.display) == -1) {
		fprintf(stderr, "err wayland roundtrip failed while creating the pointer\n");
		return 8;
	}

	printf("ready extents=%ux%u manager_version=%u outputs=%d\n", state.extent_w,
	       state.extent_h, state.manager_version, state.output_count);

	while (fgets(line, (int)sizeof(line), stdin) != NULL) {
		char *nl = strchr(line, '\n');
		if (nl != NULL) {
			*nl = '\0';
		}
		if (line[0] == '\0') {
			continue; /* blank lines are not commands and do not advance seq */
		}
		if (state.verbose) {
			fprintf(stderr, "cmd %d: %s\n", seq + 1, line);
		}
		seq++;

		char buf[sizeof(line)];
		memcpy(buf, line, strlen(line) + 1);

		char *saveptr = NULL;
		char *cmd = strtok_r(buf, " \t", &saveptr);
		if (cmd == NULL) {
			seq--;
			continue;
		}

		if (strcmp(cmd, "motion") == 0) {
			char *xs = strtok_r(NULL, " \t", &saveptr);
			char *ys = strtok_r(NULL, " \t", &saveptr);
			long x = 0;
			long y = 0;
			if (xs == NULL || ys == NULL || parse_long(xs, &x) != 0 ||
			    parse_long(ys, &y) != 0 || x < 0 || y < 0) {
				printf("err %d unknown command: %s\n", seq, line);
				fprintf(stderr, "err %d unknown command: %s\n", seq, line);
				return 9;
			}
			/*
			 * Fatal above as well as below: x >= extent_w lands off-output, so GTK would get
			 * nothing while we ack `ok` (the README's exit 10). Unsigned long compares are exact.
			 */
			if ((unsigned long)x >= (unsigned long)state.extent_w ||
			    (unsigned long)y >= (unsigned long)state.extent_h) {
				printf("err %d motion (%ld, %ld) is outside the %ux%u motion extents\n",
				       seq, x, y, state.extent_w, state.extent_h);
				fprintf(stderr,
					"err %d motion (%ld, %ld) is outside the %ux%u motion "
					"extents; x must be in [0, %u] and y in [0, %u]. The "
					"compositor normalises x/extent_w, so this point would be "
					"placed off-output and the event would reach no client "
					"while this process still reported success. Move the point "
					"inside the output, or pass -e WxH if the resolved extents "
					"are wrong.\n",
					seq, x, y, state.extent_w, state.extent_h,
					state.extent_w - 1, state.extent_h - 1);
				return 10;
			}
			zwlr_virtual_pointer_v1_motion_absolute(state.pointer, time_ms(),
								(uint32_t)x, (uint32_t)y,
								state.extent_w, state.extent_h);
			/* Required: GTK turns wl_pointer events into GdkEvents only on
			 * wl_pointer.frame (see the README's frame requirement). */
			zwlr_virtual_pointer_v1_frame(state.pointer);
		} else if (strcmp(cmd, "press") == 0 || strcmp(cmd, "release") == 0) {
			char *bs = strtok_r(NULL, " \t", &saveptr);
			long btn = 0;
			if (bs == NULL || parse_long(bs, &btn) != 0 || btn <= 0) {
				printf("err %d unknown command: %s\n", seq, line);
				fprintf(stderr, "err %d unknown command: %s\n", seq, line);
				return 9;
			}
			uint32_t btn_state = (strcmp(cmd, "press") == 0)
						     ? WL_POINTER_BUTTON_STATE_PRESSED
						     : WL_POINTER_BUTTON_STATE_RELEASED;
			zwlr_virtual_pointer_v1_button(state.pointer, time_ms(), (uint32_t)btn,
						       btn_state);
			zwlr_virtual_pointer_v1_frame(state.pointer); /* MANDATORY, see above */
		} else if (strcmp(cmd, "axis") == 0) {
			char *vs = strtok_r(NULL, " \t", &saveptr);
			char *ds = strtok_r(NULL, " \t", &saveptr);
			double value = 0.0;
			long discrete = 0;
			if (vs == NULL || parse_double(vs, &value) != 0) {
				printf("err %d unknown command: %s\n", seq, line);
				fprintf(stderr, "err %d unknown command: %s\n", seq, line);
				return 9;
			}
			if (ds != NULL) {
				if (parse_long(ds, &discrete) != 0 || discrete == 0) {
					printf("err %d unknown command: %s\n", seq, line);
					fprintf(stderr, "err %d unknown command: %s\n", seq,
						line);
					return 9;
				}
				zwlr_virtual_pointer_v1_axis_source(
					state.pointer, WL_POINTER_AXIS_SOURCE_WHEEL);
				/* axis_discrete carries both `value` and the `discrete` step count
				 * (per wlr-virtual-pointer-unstable-v1.xml), so no separate axis call. */
				zwlr_virtual_pointer_v1_axis_discrete(
					state.pointer, time_ms(),
					WL_POINTER_AXIS_VERTICAL_SCROLL,
					wl_fixed_from_double(value), (int32_t)discrete);
			} else {
				zwlr_virtual_pointer_v1_axis_source(
					state.pointer, WL_POINTER_AXIS_SOURCE_CONTINUOUS);
				zwlr_virtual_pointer_v1_axis(state.pointer, time_ms(),
							     WL_POINTER_AXIS_VERTICAL_SCROLL,
							     wl_fixed_from_double(value));
			}
			zwlr_virtual_pointer_v1_frame(state.pointer); /* MANDATORY, see above */
		} else if (strcmp(cmd, "sync") == 0) {
			/* No events; the roundtrip + ack below is the whole point. */
		} else if (strcmp(cmd, "quit") == 0) {
			zwlr_virtual_pointer_v1_destroy(state.pointer);
			wl_display_flush(state.display);
			wl_display_disconnect(state.display);
			printf("ok %d\n", seq);
			return 0;
		} else {
			printf("err %d unknown command: %s\n", seq, line);
			fprintf(stderr, "err %d unknown command: %s\n", seq, line);
			return 9;
		}

		/*
		 * The roundtrip is what makes the ack mean "the compositor has
		 * processed this event" rather than "the bytes left our buffer".
		 */
		roundtrip_or_die(seq);
		printf("ok %d\n", seq);
	}

	/* EOF on stdin. */
	zwlr_virtual_pointer_v1_destroy(state.pointer);
	wl_display_flush(state.display);
	wl_display_disconnect(state.display);
	return 0;
}
