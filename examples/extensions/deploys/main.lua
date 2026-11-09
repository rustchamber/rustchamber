local services = {
  { name = "api", state = "running", version = "v42" },
  { name = "worker", state = "running", version = "v42" },
  { name = "web", state = "deploying", version = "v43" },
}

local function count(state)
  local total = 0
  for _, service in ipairs(services) do
    if service.state == state then
      total = total + 1
    end
  end
  return total
end

local function rows()
  local result = {}
  for _, service in ipairs(services) do
    table.insert(result, { service.name, service.state, service.version })
  end
  return result
end

local function card(title, children)
  return { kind = "card", title = title, children = children }
end

function status()
  return {
    { kind = "stat", label = "Running", value = tostring(count("running")), tone = "success" },
    { kind = "stat", label = "Deploying", value = tostring(count("deploying")), delta = "web v43", tone = "info" },
    {
      kind = "table",
      columns = { "Service", "State", "Version" },
      rows = rows(),
    },
  }
end

function panel()
  return {
    { kind = "heading", text = "Deploys" },
    {
      kind = "row",
      children = {
        { kind = "badge", label = "production", tone = "info" },
        { kind = "badge", label = "fly.io", tone = "neutral" },
      },
    },
    { kind = "text", markdown = "Three services, one rollout in progress." },
    { kind = "progress", label = "Rollout", value = 66, max = 100 },
    {
      kind = "tabs",
      tabs = {
        {
          label = "Services",
          children = {
            { kind = "table", columns = { "Service", "State", "Version" }, rows = rows() },
          },
        },
        {
          label = "About",
          children = {
            { kind = "text", markdown = "This demo is fake. It exists to exercise every surface." },
            { kind = "details", title = "Manifest", children = {
              { kind = "code", language = "json", code = "capabilities: storage" },
            } },
          },
        },
      },
    },
    {
      kind = "row",
      children = {
        { kind = "button", label = "Refresh", variant = "secondary", value = "refresh" },
        { kind = "button", label = "Ask the agent", value = "ask" },
      },
    },
  }
end

function command(name, args)
  if name == "deploy" then
    local service = args:match("^(%S+)") or "web"
    host.prompt({
      text = "Deploy " .. service .. " to production. Read the deploy notes first and ask before pushing.",
      send = true,
    })
  elseif name == "ask" then
    host.prompt({
      text = "Explain what the current deploy board shows and what I should check before shipping.",
      send = true,
    })
  elseif name == "refresh" then
    return panel()
  end
end

function action(id, item)
  if id == "explain" then
    local text = ""
    if type(item) == "table" and item.text then
      text = item.text
    end
    return {
      card("Deploy explanation", {
        { kind = "text", markdown = text ~= "" and text or "Nothing selected." },
        { kind = "button", label = "Ask the agent to explain", value = "ask" },
      }),
    }
  end
end

function mentions(query)
  local result = {}
  for _, service in ipairs(services) do
    if query == "" or service.name:find(query, 1, true) then
      table.insert(result, {
        label = service.name,
        detail = service.state .. " · " .. service.version,
        text = "The " .. service.name .. " service is " .. service.state .. " (" .. service.version .. ").",
      })
    end
  end
  return result
end

function block(name, args)
  local service = args[1] or "web"
  return {
    card("Deploy " .. service, {
      { kind = "text", markdown = "This block is drawn by the extension, not by the agent." },
      { kind = "row", children = {
        { kind = "badge", label = "production", tone = "info" },
        { kind = "button", label = "Looks good", value = "Looks good" },
      } },
    }),
  }
end

function on_load()
  host.storage_set("loaded_at", "just now")
end
