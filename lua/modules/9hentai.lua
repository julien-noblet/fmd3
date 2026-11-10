function GetInfo()
	MANGAINFO.URL=MaybeFillHost(MODULE.RootURL,URL)
	if HTTP.GET(MANGAINFO.URL) then
		local x=CreateTXQuery(HTTP.Document)
		MANGAINFO.Title=x.XPathString('css("div#info > h1")')
		MANGAINFO.CoverLink=MaybeFillHost(MODULE.RootURL, x.XPathString('//div[@id="cover"]//v-lazy-image/@src'))
		MANGAINFO.Artists = x.XPathStringAll('//section[@id="tags"]/div[contains(text(), "Artist")]//a')
		MANGAINFO.Genres = x.XPathStringAll('//section[@id="tags"]/div[contains(text(), "Tag")]//a')
		MANGAINFO.ChapterLinks.Add(MANGAINFO.URL)
		MANGAINFO.ChapterNames.Add(MANGAINFO.Title)
		return no_error
	else
		return net_problem
	end
end

function GetPageNumber()
	local id = URL:match('g/(%d+)')
	HTTP.MimeType = 'application/json'
	if HTTP.POST(MODULE.RootURL..'/api/getBookByID', string.format('{"id":%s}', id)) then
		local x=CreateTXQuery(HTTP.Document)
		local srv=x.XPathString('json(*).results.image_server')
		local pages=tonumber(x.XPathString('json(*).results.total_page'))
		for i = 1, pages do
			TASK.PageLinks.Add(string.format('%s%s/%d.jpg', srv, id, i))
		end
		return true
	else
		return false
	end
end

query = '{"search":{"text":"","page":%s,"sort":0,"pages":{"range":[0,10000]},"tag":{"text":"","type":1,"tags":[],"items":{"included":[],"excluded":[]}}}}'

function GetNameAndLink()
	HTTP.MimeType = 'application/json'
	if HTTP.POST(MODULE.RootURL..'/api/getBook', string.format(query, URL)) then
		local x=CreateTXQuery(HTTP.Document)
		local v = x.XPath('json(*).results()')
		for i = 1, v.Count do
			local v1 = v.Get(i)
			LINKS.Add(string.format('%s/g/%s', MODULE.RootURL, x.XPathString('id', v1)))
			NAMES.Add(x.XPathString('title', v1))
		end
		return no_error
	else
		return net_problem
	end
end

function getdirectorypagenumber()
	HTTP.MimeType = 'application/json'
	if HTTP.POST(MODULE.RootURL..'/api/getBook', string.format(query, '0')) then
		local x = CreateTXQuery(HTTP.Document)
		PAGENUMBER = tonumber(x.XPathString('json(*).total_count')) or 1
		return true
	else
		return false
	end
end

function Init()
	local m = NewWebsiteModule()
	m.ID                       = 'f4c518f48eff41efa0929f61fdf93dc6'
	m.Category                 = 'H-Sites'
	m.Name                     = '9hentai'
	m.RootURL                  = 'https://9hentai.ru'
	m.SortedList               = true
	m.OnGetInfo                = 'GetInfo'
	m.OnGetPageNumber          = 'GetPageNumber'
	m.OnGetNameAndLink         = 'GetNameAndLink'
	m.OnGetDirectoryPageNumber = 'getdirectorypagenumber'
end
