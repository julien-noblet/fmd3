function getinfo()
	MANGAINFO.URL=MaybeFillHost(MODULE.RootURL, URL)
	if string.find(MANGAINFO.URL, '/element-list', 1, true) then
		MANGAINFO.URL = MANGAINFO.URL:gsub('/element%-list$', '')
	end
	if HTTP.GET(MANGAINFO.URL) then
		local x=CreateTXQuery(HTTP.Document)
		MANGAINFO.Title=x.XPathString('//*[@class="wrapper__heading"]/h1')
		MANGAINFO.CoverLink=MaybeFillHost(MODULE.RootURL, x.XPathString('//div[@class="make__cover"]/img/@src'))
		MANGAINFO.Authors=x.XPathStringAll('//table[@class="make__table-info"]//tr[contains(td, "Автор")]/td/a')
		MANGAINFO.Genres=x.XPathStringAll('//table[@class="make__table-info"]//tr[contains(td, "Жанры")]/td/a')
		MANGAINFO.Status = MangaInfoStatusIfPos(x.XPathString('//table[@class="make__table-info"]//tr[contains(td, "Выпуск")]/td'), 'продолжается', 'завершен')
		MANGAINFO.Summary=x.XPathString('//div[@class="make__description"]')
		if HTTP.GET(MANGAINFO.URL .. '/element-list') then
			x.ParseHTML(HTTP.Document)
			local v = x.XPath('//div[@class="post-element__description"]')
			for i = 1, v.Count do
				local v1 = v.Get(i)
				MANGAINFO.ChapterLinks.Add(x.XPathString('div[@class="post-element__action"]/a/@href', v1))
				MANGAINFO.ChapterNames.Add(x.XPathString('div[@class="post-element__meta"]', v1))
			end
			MANGAINFO.ChapterLinks.Reverse(); MANGAINFO.ChapterNames.Reverse()
			return no_error
		else
			return net_problem
		end
	else
		return net_problem
	end
end

function getpagenumber()
	TASK.PageLinks.Clear()
	TASK.PageNumber=0
	local id, chapter = URL:match('read/([^/]+)/([^/]+)/')
	local data = 'dataRun=api-manga&dataRequest=' .. id
	if HTTP.POST(MODULE.RootURL .. '/take/api-manga/request/shakai', data) then
		local x = CreateTXQuery(HTTP.Document)
		local v = x.XPath('json(*).data()[data-first="' .. chapter .. '"].data-second()')
		for i = 1, v.Count do
			local v1 = v.Get(i)
			TASK.PageLinks.Add(v1.ToString())
		end
	else
		return false
	end
	return true
end

local cataloguri = '/take/catalog/request/shakai'
function getquery(page)
	local query = 'dataRun=catalog&selectCatalog=manga&searchData=&' ..
		'selectPage=%s&' ..
		'&itemMarker=%s&' ..
		'dataModeration=&dataSorting=po-alfavitu,false,false,false,false,false,false&' ..
		'dataType=false,false,false,false,false,false,false,false&dataStatus=false,false,false,false&' ..
		'dataList=&dataGenre=false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false&dataSeason=false,false,false,false,false,false'
	return string.format(query, page, os.date("!%Y-%m-%d %X"))
end

function getnameandlink()
	if tonumber(URL) < 0 then return no_error end
	if HTTP.POST(MODULE.RootURL .. cataloguri, getquery((URL + 1))) then
		local s = HTTP.Document.ToString():gsub('&quot;', '\\"')
		local x = CreateTXQuery(s)
		local v = x.XPath('json(*).result()')
		for i = 1, v.Count do
			local v1 = v.Get(i)
			LINKS.Add(x.XPathString('output-link', v1))
			NAMES.Add(x.XPathString('output-name', v1))
		end
		return no_error
	else
		return net_problem
	end
end

function getdirectorypagenumber()
	if HTTP.POST(MODULE.RootURL .. cataloguri, getquery('1')) then
		local s = HTTP.Document.ToString():gsub('&quot;', '\\"')
		local x = CreateTXQuery(s)
		PAGENUMBER = tonumber(x.XPathString('json(*).Create')) or 1
		return no_error
	else
		return net_problem
	end
end

function Init()
	local m = NewWebsiteModule()
	m.ID = '8a1d82bce4d14cf2a42d6ef7878fb0d2'
	m.Name = 'Shakai'
	m.RootURL = 'http://shakai.ru'
	m.Category = 'Russian'
	m.LastUpdated='March 6, 2018'
	m.OnGetInfo='getinfo'
	m.OnGetPageNumber='getpagenumber'
	m.OnGetNameAndLink='getnameandlink'
	m.OnGetDirectoryPageNumber = 'getdirectorypagenumber'
end