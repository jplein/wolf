-- case: three --- lines are not doc comments and fail

--- one
--- two
--- three
local a = 1

-- case: a level-two long comment with three text lines fails

--[==[ one
two ]] still inside
three ]==]
local b = 2

-- case: delimiter-only lines of a long comment do not count

--[[
one
two
]]
local c = 3

-- case: -- inside long strings and quoted strings is not a comment

local s = [[
-- inside one
-- inside two
-- inside three
]]
local t = [=[
]] -- still inside
-- inside
-- inside
-- inside
]=]
local u = "-- seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd" .. '-- seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd'

-- case: two -- lines pass

-- one
-- two
local d = 4
