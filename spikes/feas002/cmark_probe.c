#define _POSIX_C_SOURCE 200809L

#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include "cmark-gfm-core-extensions.h"
#include "cmark-gfm.h"

static const char *EXTENSIONS[] = {
    "autolink", "strikethrough", "table", "tagfilter", "tasklist", NULL,
};

static void fail(const char *message) {
  fprintf(stderr, "%s\n", message);
  exit(1);
}

static char *read_file(const char *path, size_t *size) {
  FILE *file = fopen(path, "rb");
  char *data;
  long length;
  if (file == NULL) {
    fprintf(stderr, "failed to open %s: %s\n", path, strerror(errno));
    exit(1);
  }
  if (fseek(file, 0, SEEK_END) != 0 || (length = ftell(file)) < 0 ||
      fseek(file, 0, SEEK_SET) != 0) {
    fail("failed to determine input size");
  }
  data = malloc((size_t)length + 1);
  if (data == NULL) {
    fail("input allocation failed");
  }
  if (fread(data, 1, (size_t)length, file) != (size_t)length) {
    fail("failed to read input");
  }
  fclose(file);
  data[length] = '\0';
  *size = (size_t)length;
  return data;
}

static cmark_node *parse(const char *input, size_t size) {
  cmark_parser *parser = cmark_parser_new(CMARK_OPT_FOOTNOTES);
  const char **name;
  for (name = EXTENSIONS; *name != NULL; ++name) {
    cmark_syntax_extension *extension = cmark_find_syntax_extension(*name);
    if (extension == NULL ||
        !cmark_parser_attach_syntax_extension(parser, extension)) {
      fail("failed to attach a GFM extension");
    }
  }
  cmark_parser_feed(parser, input, size);
  cmark_node *document = cmark_parser_finish(parser);
  cmark_parser_free(parser);
  return document;
}

static size_t count_nodes(cmark_node *document, size_t *positioned) {
  size_t count = 0;
  cmark_iter *iterator = cmark_iter_new(document);
  cmark_event_type event;
  while ((event = cmark_iter_next(iterator)) != CMARK_EVENT_DONE) {
    if (event == CMARK_EVENT_ENTER) {
      cmark_node *node = cmark_iter_get_node(iterator);
      count++;
      if (cmark_node_get_start_line(node) > 0) {
        (*positioned)++;
      }
    }
  }
  cmark_iter_free(iterator);
  return count;
}

static uint64_t monotonic_ns(void) {
  struct timespec value;
  if (clock_gettime(CLOCK_MONOTONIC, &value) != 0) {
    fail("clock_gettime failed");
  }
  return (uint64_t)value.tv_sec * UINT64_C(1000000000) +
         (uint64_t)value.tv_nsec;
}

int main(int argc, char **argv) {
  size_t size;
  char *input;
  unsigned long iterations;
  size_t nodes = 0;
  size_t positioned = 0;
  uint64_t start;
  uint64_t elapsed;
  unsigned long i;

  if (argc != 3 || (iterations = strtoul(argv[2], NULL, 10)) == 0) {
    fprintf(stderr, "usage: cmark-probe <file> <iterations>\n");
    return 2;
  }
  input = read_file(argv[1], &size);
  cmark_gfm_core_extensions_ensure_registered();

  cmark_node *warmup = parse(input, size);
  positioned = 0;
  nodes = count_nodes(warmup, &positioned);
  cmark_node_free(warmup);

  start = monotonic_ns();
  for (i = 0; i < iterations; ++i) {
    cmark_node *document = parse(input, size);
    positioned = 0;
    nodes = count_nodes(document, &positioned);
    cmark_node_free(document);
  }
  elapsed = monotonic_ns() - start;
  printf("engine,file,bytes,iterations,elapsed_ns,ns_per_iteration,units,positioned_units\n");
  printf("cmark-gfm,%s,%zu,%lu,%llu,%llu,%zu,%zu\n", argv[1], size,
         iterations, (unsigned long long)elapsed,
         (unsigned long long)(elapsed / iterations), nodes, positioned);
  free(input);
  return 0;
}
