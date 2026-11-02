/*
 * libretro's log callback is printf-style (variadic). Stable Rust cannot
 * define variadic functions, so this shim formats the message and forwards
 * the finished string to a Rust sink.
 */
#include <stdarg.h>
#include <stdio.h>

typedef void (*deckard_log_sink_t)(int level, const char *msg);

static deckard_log_sink_t deckard_sink = NULL;

void deckard_log_set_sink(deckard_log_sink_t sink)
{
   deckard_sink = sink;
}

void deckard_log_printf(int level, const char *fmt, ...)
{
   char buf[2048];
   va_list ap;

   if (!deckard_sink || !fmt)
      return;

   va_start(ap, fmt);
   vsnprintf(buf, sizeof(buf), fmt, ap);
   va_end(ap);

   deckard_sink(level, buf);
}
