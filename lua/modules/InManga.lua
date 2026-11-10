function getinfo()
	MANGAINFO.URL=MaybeFillHost(MODULE.RootURL, URL)
	if HTTP.GET(MANGAINFO.URL) then
		local x=CreateTXQuery(HTTP.Document)
		MANGAINFO.Title=x.XPathString('//h1')
		MANGAINFO.CoverLink=MaybeFillHost(MODULE.RootURL, x.XPathString('//meta[@property="og:image"]/@content'))
		MANGAINFO.Summary=x.XPathString('//div/div[h1]/following-sibling::div[@class="panel-body"]')
		local id = x.XPathString('//input[@id="Identification"]/@value')
		local base = x.XPathString('//script[contains(., "var chapterUrl")]')
		base = GetBetween("'", "'", base)
		if HTTP.GET(MODULE.RootURL .. '/chapter/getall?mangaIdentification=' .. id) then
			x.ParseHTML(HTTP.Document)
			local root = x.XPath('json(json(*).data)')
			local v = x.XPath('jn:members(result)', root)
			local t = {}
			for i = 1, v.Count do
				table.insert(t, tonumber(x.XPathString('Number', v.Get(i))))
			end
			table.sort(t)
			for _, k in ipairs(t) do
				local chid = x.XPathString('jn:members(result)[Number='..k..']/Identification', root)
				local chnum = x.XPathString('jn:members(result)[Number='..k..']/FriendlyChapterNumber', root)
				MANGAINFO.ChapterLinks.Add(MODULE.RootURL .. '/chapter/chapterIndexControls?identification=' .. chid)
				MANGAINFO.ChapterNames.Add('Capítulo: ' .. chnum)
			end
		end
		return no_error
	else
		return net_problem
	end
end

function getpagenumber()
	TASK.PageLinks.Clear()
	if HTTP.GET(MaybeFillHost(MODULE.RootURL, URL)) then
		local x=CreateTXQuery(HTTP.Document)
		local v=x.XPath('//div[contains(@class,"PagesContainer")]/a/img/@id')
		for i = 1, v.Count do
			local id = v.Get(i).ToString()
			TASK.PageLinks.Add(MaybeFillHost(MODULE.RootURL, '/page/getPageImage/?identification='..id));
		end
	else
		return false
	end
	return true
end

function getnameandlink()
	local data = 'filter%5Bgeneres%5D%5B%5D=-1&filter%5BqueryString%5D=&filter%5Bskip%5D=0&filter%5Btake%5D=10000&filter%5Bsortby%5D=5&filter%5BbroadcastStatus%5D=0'
	if HTTP.POST(MODULE.RootURL .. '/manga/getMangasConsultResult', data) then
		local x = CreateTXQuery(HTTP.Document)
		local v = x.XPath('//a[contains(@class,"manga-result")]')
		for i = 1, v.Count do
			local v1 = v.Get(i)
			LINKS.Add(v1.GetAttribute('href'))
			NAMES.Add(x.XPathString('.//h4', v1))
		end
		return no_error
	else
		return net_problem
	end
end

function Init()
	local m = NewWebsiteModule()
	m.ID = '03ae42ac9141443fa0711bd401c0203d'
	m.Name = 'InManga'
	m.RootURL = 'https://inmanga.com'
	m.Category = 'Spanish'
	m.LastUpdated='April 16, 2018'
	m.OnGetInfo='getinfo'
	m.OnGetPageNumber='getpagenumber'
	m.OnGetNameAndLink='getnameandlink'
end
