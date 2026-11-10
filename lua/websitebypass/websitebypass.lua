-- don't change the function name(hardcoded, case sensitive)
-- don't change the first 2 parameters method and url(hardcoded)
-- must return 1 boolean value
function ____WebsiteBypass(METHOD, URL)
	local server = HTTP.Headers.Values['Server']:lower()
	local bypass
	if server:find('cloudflare') then
		bypass = require 'websitebypass.cloudflare'
	elseif server:find('ddos%-guard') then
		bypass = require 'websitebypass.ddos-guard'
	end
	if bypass ~= nil then
		LOGGER = require 'fmd.logger'
		return bypass:bypass(METHOD, URL)
	end
	return false
end