function Init()
	local m = NewWebsiteModule()
	m.ID                         = 'c67d163c51b24bc487e777e2b0d810d2'
	m.Name                       = 'Gntai'
	m.RootURL                    = 'http://www.gntai.net'
	m.Category                   = 'H-Sites'
	m.OnGetDirectoryPageNumber   = 'GetDirectoryPageNumber'
	m.OnGetNameAndLink           = 'GetNameAndLink'
	m.OnGetInfo                  = 'GetInfo'
	m.OnGetPageNumber            = 'GetPageNumber'
	m.SortedList                 = true
end

function GetDirectoryPageNumber()
	if HTTP.GET(MODULE.RootURL) then
		PAGENUMBER = tonumber(CreateTXQuery(HTTP.Document).XPathString('//div[@class="navigation"]//li[last()]/a')) or 1
		return no_error
	else
		return net_problem
	end
end

function GetNameAndLink()
	if HTTP.GET(MODULE.RootURL .. '/page/' .. (URL + 1)) then
		CreateTXQuery(HTTP.Document).XPathHREFTitleAll('//*[@id="main"]//div[contains(@class, "chapter-thumb")]/a', LINKS, NAMES)
		return no_error
	else
		return net_problem
	end
end

function GetInfo()
	MANGAINFO.URL = MaybeFillHost(MODULE.RootURL, URL)
	if HTTP.GET(MANGAINFO.URL) then
		local x = CreateTXQuery(HTTP.Document)
		MANGAINFO.Authors    = x.XPathString('//div[@class="uk-text-center"]/h2/a')
		MANGAINFO.Title      = '[' .. MANGAINFO.Authors .. '] ' .. x.XPathString('//div[@class="uk-text-center"]/h1')
		MANGAINFO.CoverLink  = x.XPathString('//div[@id="img-page"]//img/@src')
		MANGAINFO.Genres     = x.XPathStringAll('//div[contains(@class, "generos-tags")]/a')

		MANGAINFO.ChapterLinks.Add(URL)
		MANGAINFO.ChapterNames.Add(MANGAINFO.Title)
		return no_error
	else
		return net_problem
	end
end

function GetPageNumber()
	if HTTP.GET(MaybeFillHost(MODULE.RootURL, URL)) then
		local x = CreateTXQuery(HTTP.Document)
		local s = x.XPathString('//script[contains(., "var pages")]')
		s = "["..GetBetween("var pages = [", "]", s).."]"
		x.ParseHTML(s)
		x.XPathStringAll('json(*)()("page_image")', TASK.PageLinks)
		return true
	else
		return false
	end
end