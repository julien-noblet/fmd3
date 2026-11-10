-- don't change the function name (hardcoded, case sensitive)
-- must return 1 boolean value
-- this function will be called very often(for each of http request), keep it minimal
function ____CheckAntiBot(HTTP)
	local rc = HTTP.ResultCode
	if ((rc == 403) or (rc == 429) or (rc == 503)) and HTTP.Headers.Values['Content-Type']:lower():find('text/html') then
		local server = HTTP.Headers.Values['Server']:lower()
		if server:find('cloudflare') or server:find('ddos%-guard') then
			return true
		end
	end
	return false
end