function Init()
	local m = NewWebsiteModule()
	m.ID                         = '2ca5fae1c28f4c4db4ecc71377da5bac'
	m.Name                       = 'SundayWebEvery'
	m.RootURL                    = 'https://www.sunday-webry.com'
	m.Category                   = 'Raw'
	m.OnGetNameAndLink           = 'GetNameAndLink'
	m.OnGetInfo                  = 'GetInfo'
	m.OnGetPageNumber            = 'GetPageNumber'
end

function GetNameAndLink()
	if HTTP.GET(MODULE.RootURL .. '/comics/') then
		CreateTXQuery(HTTP.Document).XPathHREFAll('//ul[@class="manga-list__list"]/li/h4/a', LINKS, NAMES)
		return no_error
	else
		return net_problem
	end
end

function GetInfo()
	MANGAINFO.URL = MaybeFillHost(MODULE.RootURL, URL):gsub('/+$', '') .. '/'
	if HTTP.GET(MANGAINFO.URL) then
		local x = CreateTXQuery(HTTP.Document)

		MANGAINFO.CoverLink = MaybeFillHost(MODULE.RootURL, x.XPathString('//*[@id="mainvisual"]/img/@src'))
		MANGAINFO.Title     = x.XPathString('//*[@class="title"]/h1')
		MANGAINFO.Summary   = x.XPathString('//*[@class="title"]/h2')

		-- there is no chapter list?
		-- assuming the first chapter link in manga info is always the last chapters
		local s, last_eps = x.XPathString('//article/a[1]/@href'):match('^(.-)(%d+)/?$') or '', '0'
		local n
		if (tonumber(last_eps) > 0) and (s ~= '') then
			local i for i = 1, tonumber(last_eps) do
				n = string.format('%.3d', i)
				MANGAINFO.ChapterLinks.Add(s .. n)
				MANGAINFO.ChapterNames.Add(n)
			end
		end
		return no_error
	else
		return net_problem
	end
end

function GetPageNumber()
	if HTTP.GET(MaybeFillHost(Module.RootURL, URL):gsub('/+$', '') .. '/') then
		local key = HTTP.Document.ToString():match('.-key:%s*.-["\'](.-)["\']')
		if (key ~= nil) or (key ~= '') then
			key = MODULE.RootURL .. '/assets/episodes/' .. key .. '/'
			if HTTP.GET(key .. 'episode.json') then
				local v for v in CreateTXQuery(HTTP.Document).XPath('json(*).pages()/files/h1536.jpeg').Get() do
					TASK.PageLinks.Add(key .. v.ToString())
				end
			end
		end
		return true
	else
		return false
	end
end
