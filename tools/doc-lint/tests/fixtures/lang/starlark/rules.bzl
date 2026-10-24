# case: two # lines pass
# (the second line of that block)

def documented():
    """Docstring with one " quote, then # lines.

    # not a comment one
    # not a comment two
    # not a comment three
    """
    single = ''' one ' quote
# not a comment one
# not a comment two
# not a comment three
'''
    return [single, '# seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd', "# seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd", r"# seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd", b'# seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd']

# case: three # lines fail

# one
# two
# three
x = 1
