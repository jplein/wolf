// case: an interpolation holding a string with // is not a comment

let a = "\(f("// seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd"))"
let b = "\(g((1), "// seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd"))"

// case: // lines inside a multi-line string holding a quote are not comments

let c = """
    one " quote
    // x
    // x
    // x
    """

// case: raw strings keep quotes and // inside, including multi-line raw strings

let d = #"a quote " then // seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd"#
let e = #"""
    one " quote \#(d)
    // x
    // x
    // x
    """#

// case: nested block comments; the inner */ does not close the outer

/* outer /* inner */
   // still outer */
// real one
// real two
func afterNested() {}

// case: three // lines after all of the above still fail

// tail one
// tail two
// tail three
func tail() {}
