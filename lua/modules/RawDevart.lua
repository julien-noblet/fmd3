function getinfo()
	local x = nil
		local u = MaybeFillHost(MODULE.RootURL, URL)

		--debug URL manga info
		if not HTTP.GET(u) then return net_problem end

		x = CreateTXQuery(HTTP.Document)
		MANGAINFO.CoverLink	= MaybeFillHost(MODULE.RootURL, x.XPathString('//*[@class="img-fluid not-lazy"]/@src'))
		MANGAINFO.Title     = x.XPathString('//h1[contains(@class, "title")]')
		MANGAINFO.Genres    = x.XPathStringAll('//*[contains(@class, "genres")]/a')
		-- MANGAINFO.Status	= MangaInfoStatusIfPos()
		MANGAINFO.Summary	= x.XPathString('//*[contains(@class, "description")]'):gsub('Description', '')

		local pages = 1
		local p = 1
		while p <= pages do
			if p > 1 then
				if HTTP.GET(MANGAINFO.URL .. '?page=' .. tostring(p)) then
					x=CreateTXQuery(HTTP.Document)
				else
					break
				end
			end
			if p == pages then
				local pg = x.XPathString('//*[contains(@class, "pagination")]/li[last()]/a/substring-after(@href, "?page=")')
				if pg ~= '' then pages = tonumber(pg) end
			end
			local v=x.XPath('//div[contains(@class, "list-group")]//div')
			for i=1,v.Count do
				local v1=v.Get(i)
				MANGAINFO.ChapterNames.Add(Trim(x.XPathString('.//a/span', v1)))
			MANGAINFO.ChapterLinks.Add(x.XPathString('.//a/@href', v1));
			end
			p = p + 1
		end
		MANGAINFO.ChapterLinks.Reverse(); MANGAINFO.ChapterNames.Reverse()

		return no_error
end

function getpagenumber()
	local x = nil
	local u = MaybeFillHost(MODULE.RootURL, URL)
	if not HTTP.GET(u) then return net_problem end
	x = CreateTXQuery(HTTP.Document)
	x.XPathStringAll('//*[contains(@class, "mb-3")]/img/@data-src', TASK.PageLinks)
	if TASK.PageLinks.Count < 1 then x.XPathStringAll('//*[contains(@class, "img-fluid not-lazy")]/@data-src', TASK.PageLinks) end
	return no_error
end

function getnameandlink()
	local x = nil
	local u = MODULE.RootURL .. '/comic/'
	if not HTTP.GET(u) then return net_problem end
	x = CreateTXQuery(HTTP.Document)
	local pages = 1
		local p = 1
		while p <= pages do
			if p > 1 then
				if HTTP.GET(u .. '?page=' .. tostring(p)) then
					x=CreateTXQuery(HTTP.Document)
				else
					break
				end
			end
			if p == pages then
				local pg = x.XPathString('//*[contains(@class, "pagination")]/li[last()]/a/substring-after(@href, "?page=")')
				if pg ~= '' then pages = tonumber(pg) end
			end
			local v=x.XPath('//div[contains(@class, "row mb-3")]//div')
			for i=1,v.Count do
				local v1=v.Get(i)
				NAMES.Add(Trim(x.XPathString('.//*[contains(@class, "title")]/span', v1)))
				LINKS.Add(x.XPathString('.//a[contains(@class, "head")]/@href', v1));
			end
			p = p + 1
		end
end

function Init()
	local m = NewWebsiteModule()
	m.ID                       = '97cad215aa8d466abe47a888f2892e17'
	m.Name                     = 'RawDevart'
	m.RootURL                  = 'https://rawdevart.com'
	m.Category                 = 'Raw'
	m.OnGetInfo                = 'getinfo'
	m.OnGetNameAndLink         = 'getnameandlink'
	m.OnGetPageNumber          = 'getpagenumber'
end
