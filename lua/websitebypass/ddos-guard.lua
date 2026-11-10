local _m = {}

function _m.getDDoSGuardCookie(self, url)
	local body = HTTP.Document.ToString()

	if body:find('://check.ddos-guard.net/check.js', 1, true) then
		local result = false
		HTTP.EnabledCookies = false
		if HTTP.Request('POST', 'https://check.ddos-guard.net/check.js') then
			local headers = HTTP.Headers.Text
			if headers:find('Set%-Cookie:[^\r\n]*__ddg') then
				local domain = url:match('https?://([^/]+)') or ''
				headers = headers:gsub('check%.ddos%-guard%.net', domain)
				HTTP.Headers.Text = headers
				HTTP.ParseServerCookies()
				result = true
			end
		end
		HTTP.EnabledCookies = true
		return result
	end

	LOGGER.SendWarning('WebsitBypass[ddos-guard]: no DDoS-GUARD solution found!\r\n' .. url)
	return false
end

function _m.bypass(self, METHOD, URL)
	local result = false
	local counter = 0
	local maxretry = HTTP.RetryCount;
	HTTP.RetryCount = 0

	while true do
		counter = counter + 1
		result = self:getDDoSGuardCookie(URL)
		if result then
			result = HTTP.Request(METHOD, URL)
		end
		if result then break end
		if HTTP.Terminated then break end
		-- delay before retry
		self:sleepOrBreak(2000)
		if (maxretry > -1) and (maxretry <= counter) then break end
		HTTP.Reset()
		HTTP.Request('GET', URL)
	end

	HTTP.RetryCount = maxretry
	return result
end

return _m