// case: a ten-line //! comment with 150-char lines is exempt

//! seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
//! seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
//! seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
//! seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
//! seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
//! seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
//! seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
//! seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
//! seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
//! seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd

// case: a ten-line /// comment with 150-char lines is exempt

/// seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
/// seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
/// seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
/// seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
/// seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
/// seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
/// seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
/// seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
/// seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
/// seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
fn outer_doc() {}

// case: a ten-line /** */ comment with 150-char lines is exempt

/** seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
 * seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
 * seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
 * seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
 * seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
 * seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
 * seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
 * seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
 * seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
 * seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd */
fn block_doc() {}

// case: a ten-line /*! */ comment with 150-char lines is exempt

/*! seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
 * seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
 * seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
 * seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
 * seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
 * seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
 * seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
 * seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
 * seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd
 * seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd seg11abcd seg12abcd seg13abcd seg14abcd seg15abcd */
fn inner_block_doc() {}

// case: //// is not a doc comment, so three lines fail comment-lines

//// one
//// two
//// three
fn quad() {}

// case: /**/ is an empty plain block, so the three // lines after it fail

/**/
// one
// two
// three
fn empty_block() {}

// case: /*** opens a plain block, so three text lines fail comment-lines

/*** banner one
     banner two
     banner three */
fn banner() {}

// case: a /// line then three // lines reports only the // block

/// doc line
// plain one
// plain two
// plain three
fn families() {}
