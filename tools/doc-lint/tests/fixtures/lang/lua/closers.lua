-- case: an opener-only line and a dash-prefixed closer-only line do not count

--[[
one
two
--]]
local a = 1

-- case: three text lines still fail with a dash-prefixed closer-only line

--[[
one
two
three
--]]
local b = 2

-- case: text before the dash-prefixed closer counts

--[[
one
two
three --]]
local c = 3

-- case: a level-2 opener-only line and dash-prefixed closer-only line do not count

--[==[
one
two
--]==]
local d = 4

-- case: three text lines still fail with a level-2 dash-prefixed closer-only line

--[==[
one
two
three
--]==]
local e = 5
