-- `nvim -l` probe for queries_compile_test.sh: per language, compiles the vendored nvim-treesitter
-- highlights (and injections, if any) against our parser. Args: <ts_dir> <parser_dir> <lang>...

local ts_dir, parser_dir = arg[1], arg[2]
local langs = {}
for i = 3, #arg do
  langs[#langs + 1] = arg[i]
end

-- Flushed per line, so a later crash cannot swallow the verdicts already printed.
local function out(line)
  io.stdout:write(line, "\n")
  io.stdout:flush()
end

if not ts_dir or not parser_dir or #langs == 0 then
  out("FAIL usage: <ts_dir> <parser_dir> <lang>... (got " .. #arg .. " args)")
  os.exit(2)
end

local ts_stat = vim.uv.fs_stat(ts_dir)
if not ts_stat or ts_stat.type ~= "directory" then
  out("FAIL nvim-treesitter dir does not exist: " .. ts_dir)
  os.exit(2)
end
local ts_norm = (vim.fs.normalize(ts_dir):gsub("/+$", ""))
vim.opt.runtimepath:prepend(ts_norm)

-- nvim's runtime ships its own queries for some languages (c); a pass must use the vendored tree.
-- Unresolved paths: a sandboxed tree holds per-file symlinks whose realpaths leave ts_dir.
local function any_from_ts_tree(files)
  for _, f in ipairs(files) do
    if vim.fs.normalize(f):sub(1, #ts_norm + 1) == ts_norm .. "/" then
      return true
    end
  end
  return false
end

-- query.get raises on a compile error (e.g. an unknown node type) and returns nil with no files.
local function compile(lang, name)
  local ok, query = pcall(vim.treesitter.query.get, lang, name)
  if not ok then
    return name .. " failed to compile: " .. tostring(query)
  end
  if query == nil then
    return name .. " query is nil (no query files resolved)"
  end
  return nil
end

local function check(lang)
  local so = parser_dir .. "/" .. lang .. ".so"
  -- Since Neovim 0.11, add returns true, or nil plus an error (ABI out of range, missing symbol).
  local ok, added, add_err = pcall(vim.treesitter.language.add, lang, { path = so })
  if not ok then
    return "language.add raised for " .. so .. ": " .. tostring(added)
  end
  if not added then
    return "language.add did not load " .. so .. ": " .. tostring(add_err)
  end

  local hl_files = vim.treesitter.query.get_files(lang, "highlights")
  if not any_from_ts_tree(hl_files) then
    return "no highlights query under "
      .. ts_norm
      .. " (resolved: "
      .. (#hl_files > 0 and table.concat(hl_files, ", ") or "none")
      .. ")"
  end
  local err = compile(lang, "highlights")
  if err then
    return err
  end

  if #vim.treesitter.query.get_files(lang, "injections") > 0 then
    err = compile(lang, "injections")
    if err then
      return err
    end
  end
  return nil
end

local failed = 0
for _, lang in ipairs(langs) do
  local ok, err = pcall(check, lang)
  if not ok then
    err = "probe raised: " .. tostring(err)
  end
  if err then
    failed = failed + 1
    out("FAIL " .. lang .. ": " .. err)
  else
    out("PASS " .. lang)
  end
end

out(("SUMMARY %d/%d languages failed"):format(failed, #langs))
os.exit(failed > 0 and 1 or 0)
