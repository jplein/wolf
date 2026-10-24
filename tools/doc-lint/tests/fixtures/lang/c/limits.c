// case: /** */ is not a doc comment in C, so three text lines fail

/** one
 * two
 * three */
int doc_like;

// case: // inside a string and '/' as a char are not comments

const char *url = "http://example.com/ seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd";
char slash = '/'; const char *s2 = "// seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd";

// case: C blocks do not nest, so the first */ closes the comment

/* one /* two */
int after_block;
// c one
// c two
// c three

// case: a continuation line is measured from its first non-space char

/* opener
 * seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abc
 */
/* opener
 * seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd
 */
int after_width;
