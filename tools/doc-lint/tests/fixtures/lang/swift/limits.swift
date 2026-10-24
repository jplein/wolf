// case: /// and /** */ doc comments are exempt, while three // lines fail

/// seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
/// seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
/// seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
func documented() {}

/** seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
 * seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
 * seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd */
func blockDocumented() {}

// one
// two
// three
func plain() {}

// case: #if is a compiler directive, not a raw string opener

#if DEBUG
// debug one
// debug two
// debug three
#endif
